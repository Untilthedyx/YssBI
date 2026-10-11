import importlib.util
import os
from pathlib import Path
import shutil
import stat
import sys
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("package_linux_native", SCRIPTS / "package_linux_native.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)

import linux_package_smoke as smoke


class NativeLinuxTests(unittest.TestCase):
    def test_native_asset_names(self):
        for architecture in ("x86_64", "aarch64"):
            with self.subTest(architecture=architecture):
                self.assertEqual(
                    native.native_asset_name("1.2.3-rc.2+build.7", architecture, "deb"),
                    f"yssbi-1.2.3-rc.2+build.7-ubuntu24.04-{architecture}.deb",
                )
                self.assertEqual(
                    native.native_asset_name("1.2.3", architecture, "rpm"),
                    f"yssbi-1.2.3-linux-{architecture}.rpm",
                )
        for architecture in ("amd64", "arm64", "i686", "../x86_64"):
            with self.subTest(architecture=architecture), self.assertRaises(ValueError):
                native.native_asset_name("1.2.3", architecture, "deb")
        with self.assertRaisesRegex(ValueError, "format"):
            native.native_asset_name("1.2.3", "x86_64", "tar.gz")

    def test_exact_distribution_baselines(self):
        native.validate_distribution("deb", {"ID": "ubuntu", "VERSION_ID": "24.04"})
        native.validate_distribution("rpm", {"ID": "ubuntu", "VERSION_ID": "24.04"})
        native.validate_distribution("rpm", {"ID": "fedora", "VERSION_ID": "44"})
        for package_format, release in (
            ("deb", {"ID": "ubuntu", "VERSION_ID": "22.04"}),
            ("deb", {"ID": "debian", "VERSION_ID": "13"}),
            ("deb", {"ID": "mint", "ID_LIKE": "ubuntu", "VERSION_ID": "24.04"}),
            ("rpm", {"ID": "fedora", "VERSION_ID": "43"}),
            ("rpm", {"ID": "ubuntu", "VERSION_ID": "22.04"}),
            ("rpm", {"ID": "ubuntu", "VERSION_ID": "26.04"}),
            ("rpm", {"ID": "mint", "ID_LIKE": "ubuntu", "VERSION_ID": "24.04"}),
            ("rpm", {"ID": "rhel", "VERSION_ID": "44"}),
            ("rpm", {}),
        ):
            with self.subTest(package_format=package_format, release=release):
                with self.assertRaisesRegex(ValueError, "requires"):
                    native.validate_distribution(package_format, release)

    def test_wrong_distribution_fails_before_tools_or_output_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "output"
            with patch.object(native.platform, "freedesktop_os_release", return_value={
                "ID": "fedora", "VERSION_ID": "44",
            }), patch.object(native, "_run") as run:
                with self.assertRaisesRegex(ValueError, "requires ubuntu 24.04"):
                    native.package_native_linux(
                        "missing-binary", "missing-samples", "1.2.3", "x86_64",
                        output, "missing-notices", "deb",
                    )
                run.assert_not_called()
            self.assertFalse(output.exists())

    def test_semver_conversion_and_injection_rejection(self):
        for package_format in ("deb", "rpm"):
            self.assertEqual(native.package_version("1.2.3", package_format), "1.2.3")
            self.assertEqual(native.package_version("1.2.3-rc.2+build.15", package_format), "1.2.3~rc.2")
            self.assertEqual(native.package_version("1.2.3+build.15", package_format), "1.2.3")
            for invalid in (
                "1.2", "01.2.3", "1.02.3", "1.2.03", "1.2.3-01", "1.2.3-rc.01",
                "1.2.3-", "1.2.3-a..b", "1.2.3+", "1.2.3+x..y", "1.2.3\n",
                "1.2.3\nRequires: evil", "1.2.3-%{lua:evil}", "1.2.3/../../escape",
            ):
                with self.subTest(package_format=package_format, version=invalid), self.assertRaises(ValueError):
                    native.package_version(invalid, package_format)
        self.assertEqual(native.package_version("1.2.3-alpha-beta", "deb"), "1.2.3~alpha-beta")
        self.assertEqual(native.package_version("1.2.3-alpha-beta", "rpm"), "1.2.3~alpha.beta")
        self.assertEqual(native.package_version("0.0.0+001", "deb"), "0.0.0")

    @unittest.skipUnless(shutil.which("dpkg"), "dpkg version comparator is unavailable")
    def test_debian_prerelease_upgrade_order(self):
        versions = ("1.2.3-alpha.1", "1.2.3-beta.1", "1.2.3-rc.2", "1.2.3-rc.10", "1.2.3", "1.2.4")
        for before, after in zip(versions, versions[1:]):
            with self.subTest(before=before, after=after):
                subprocess.run([
                    "dpkg", "--compare-versions", native.package_version(before, "deb") + "-1",
                    "lt", native.package_version(after, "deb") + "-1",
                ], check=True, capture_output=True, text=True)

    @unittest.skipUnless(shutil.which("rpm"), "RPM version comparator is unavailable")
    def test_rpm_prerelease_upgrade_order(self):
        versions = ("1.2.3-alpha.1", "1.2.3-beta.1", "1.2.3-rc.2", "1.2.3-rc.10", "1.2.3", "1.2.4")
        for before, after in zip(versions, versions[1:]):
            with self.subTest(before=before, after=after):
                left = native.package_version(before, "rpm")
                right = native.package_version(after, "rpm")
                result = subprocess.run([
                    "rpm", "--eval", f'%{{lua: print(rpm.vercmp("{left}", "{right}"))}}',
                ], check=True, capture_output=True, text=True)
                self.assertEqual(result.stdout.strip(), "-1")

    @unittest.skipUnless(os.name == "posix", "Native Unix file permissions are required")
    def test_staging_is_readable_with_restrictive_umask_without_changing_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            binary = source / "binary"
            binary.write_bytes(b"executable fixture")
            binary.chmod(0o700)
            notices = source / "notices"
            notices.write_text("Third-party notices", encoding="utf-8")
            samples = source / "samples"
            payload = samples / "iris/v1/data.parquet"
            payload.parent.mkdir(parents=True)
            payload.write_bytes(b"PAR1\x00\xffunchanged sample bytesPAR1")
            payload.chmod(0o600)
            (samples / "README.md").write_text("Sample attribution", encoding="utf-8")
            (samples / "catalog.json").write_text("{}", encoding="utf-8")
            (samples / "sources.json").write_text("{}", encoding="utf-8")
            root = source / "payload"
            previous = os.umask(0o077)
            try:
                packaged_binary = native._stage_payload(binary, samples, notices, root, "deb")
            finally:
                os.umask(previous)
            self.assertEqual(stat.S_IMODE(binary.stat().st_mode), 0o700)
            self.assertEqual(stat.S_IMODE(payload.stat().st_mode), 0o600)
            for path in (root, *root.rglob("*")):
                if path.is_symlink():
                    continue
                expected = 0o755 if path.is_dir() or path == packaged_binary else 0o644
                with self.subTest(path=path.relative_to(root)):
                    self.assertEqual(stat.S_IMODE(path.stat().st_mode), expected)

    @unittest.skipUnless(os.name == "posix", "Native Unix symlinks are required")
    def test_resource_symlinks_cannot_escape_staging(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            outside = source / "private"
            outside.write_bytes(b"private data")
            samples = source / "samples"
            samples.mkdir()
            (samples / "escape").symlink_to(outside)
            with self.assertRaisesRegex(ValueError, "regular files"):
                native._copy_data_tree(samples, source / "staged")
            self.assertFalse((source / "staged/escape").exists())
            self.assertEqual(outside.read_bytes(), b"private data")

    @unittest.skipUnless(os.name == "posix", "Native Unix special files are required")
    def test_resource_special_files_and_shared_libraries_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index, filename in enumerate(("libextra.so", "libextra.so.1", "pipe")):
                samples = root / f"samples-{index}"
                samples.mkdir()
                child = samples / filename
                if filename == "pipe":
                    os.mkfifo(child)
                else:
                    child.write_bytes(b"library")
                with self.subTest(filename=filename), self.assertRaises(ValueError):
                    native._copy_data_tree(samples, root / f"staged-{index}")

    def test_archive_rejects_paths_and_types_that_escape_rpm_metadata(self):
        regular = stat.S_IFREG | 0o755
        link = stat.S_IFLNK | 0o777
        for name, entry_type, target, mode, size, uid in (
            ("../outside", tarfile.REGTYPE, "", regular, 0, 0),
            ("/outside", tarfile.REGTYPE, "", regular, 0, 0),
            (str(smoke.BINARY), tarfile.LNKTYPE, "/outside", regular, 0, 0),
            (str(smoke.BINARY), tarfile.FIFOTYPE, "", regular, 0, 0),
            (str(smoke.BINARY), tarfile.REGTYPE, "", regular, 1, 0),
            (str(smoke.BINARY), tarfile.REGTYPE, "", regular, 0, 1000),
            (str(smoke.LAUNCHER), tarfile.SYMTYPE, "/outside", link, 0, 0),
        ):
            entry = tarfile.TarInfo(name)
            entry.type, entry.linkname = entry_type, target
            entry.mode, entry.size, entry.uid = stat.S_IMODE(mode), size, uid
            expected_path = smoke.LAUNCHER if mode == link else smoke.BINARY
            metadata = {expected_path: (mode, len(smoke.LINK_TARGET) if mode == link else 0)}
            with self.subTest(name=name, entry_type=entry_type, size=size, uid=uid):
                with self.assertRaises(ValueError):
                    smoke.validate_archive([entry], metadata)

    def test_archive_rejects_missing_and_duplicate_payload_entries(self):
        entry = tarfile.TarInfo(str(smoke.BINARY))
        entry.mode = 0o755
        metadata = {smoke.BINARY: (stat.S_IFREG | 0o755, 0)}
        for entries in ([], [entry, entry]):
            with self.subTest(entries=len(entries)), self.assertRaises(ValueError):
                smoke.validate_archive(entries, metadata)


if __name__ == "__main__":
    unittest.main()
