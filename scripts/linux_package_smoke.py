#!/usr/bin/env python3
"""Inspect RPM contents or exercise native installation on an ephemeral clean host.

Use rpm-contents without root on Ubuntu 24.04 or Fedora 44. Installation modes
require root: deb on Ubuntu 24.04, rpm on Fedora 44. They refuse prior installs.
"""

import argparse
import configparser
import json
import os
from pathlib import Path, PurePosixPath
import platform
import stat
import subprocess
import tarfile
import tempfile

from package import sha256, verify_elf, verify_samples
from package_linux_native import (
    ARCHITECTURES, DEB_RUNTIME_DEPENDENCIES, RPM_RUNTIME_DEPENDENCIES,
    native_asset_name, package_version,
)

BINARY = PurePosixPath("usr/libexec/yssbi/yss-desktop-gpui")
LAUNCHER = PurePosixPath("usr/bin/yssbi")
DESKTOP = PurePosixPath("usr/share/applications/com.zjy.yssbi.desktop")
SAMPLES = BINARY.parent / "resources/samples"
RPM_NOTICE = PurePosixPath("usr/share/doc/yssbi/THIRD_PARTY_NOTICES.txt")
LINK_TARGET = "../libexec/yssbi/yss-desktop-gpui"


def run(*arguments, **kwargs):
    print("+", " ".join(map(str, arguments)), flush=True)
    return subprocess.run(list(map(str, arguments)), check=True, **kwargs)


def capture(*arguments):
    return run(*arguments, stdout=subprocess.PIPE, text=True).stdout


def asset_version(asset, package_format, architecture):
    suffix = (f"-ubuntu24.04-{architecture}.deb" if package_format == "deb"
              else f"-linux-{architecture}.rpm")
    if not asset.name.startswith("yssbi-") or not asset.name.endswith(suffix):
        raise ValueError(f"Unexpected native asset filename: {asset.name}")
    version = asset.name[len("yssbi-"):-len(suffix)]
    if native_asset_name(version, architecture, package_format) != asset.name:
        raise ValueError("Native asset filename does not match its version")
    return package_version(version, package_format)


def verify_payload(root, package_format, architecture):
    binary, launcher, desktop = (root / path for path in (BINARY, LAUNCHER, DESKTOP))
    notice_path = RPM_NOTICE if package_format == "rpm" else RPM_NOTICE.with_name("copyright")
    notices = root / notice_path
    if not launcher.is_symlink() or os.readlink(launcher) != LINK_TARGET:
        raise ValueError("Launcher must be the expected relative symlink")
    if launcher.resolve(strict=True) != binary:
        raise ValueError("Launcher does not resolve to the native executable")
    verify_elf(binary, architecture)
    verify_samples(root / SAMPLES)
    catalog = json.loads((root / SAMPLES / "catalog.json").read_text(encoding="utf-8"))
    sample_files = {SAMPLES / name for name in ("catalog.json", "README.md", "sources.json")}
    for dataset in catalog["datasets"]:
        path = SAMPLES / dataset["id"] / f"v{dataset['version']}" / "data.parquet"
        if path in sample_files:
            raise ValueError("Duplicate sample catalog entry")
        sample_files.add(path)
    expected_files = sample_files | {BINARY, LAUNCHER, DESKTOP, notice_path}
    expected_directories = {BINARY.parent, notice_path.parent}
    for path in sample_files:
        expected_directories.update(parent for parent in path.parents if parent.is_relative_to(BINARY.parent))
    actual = {PurePosixPath(path.relative_to(root).as_posix())
              for directory in (binary.parent, notices.parent) for path in directory.rglob("*")}
    actual.update((BINARY.parent, notice_path.parent, LAUNCHER, DESKTOP))
    if actual != expected_files | expected_directories:
        raise ValueError(f"Unexpected or missing payload paths: {actual ^ (expected_files | expected_directories)}")
    for relative in expected_files | expected_directories:
        mode = (root / relative).lstat().st_mode
        expected_mode = (stat.S_IFLNK | 0o777 if relative == LAUNCHER else
                         stat.S_IFDIR | 0o755 if relative in expected_directories else
                         stat.S_IFREG | (0o755 if relative == BINARY else 0o644))
        if mode != expected_mode:
            raise ValueError(f"Unexpected payload file type or permissions: {relative}")
    if not notices.stat().st_size:
        raise ValueError("Missing third-party notices")
    entry = configparser.ConfigParser(interpolation=None)
    entry.read(desktop, encoding="utf-8")
    if (entry["Desktop Entry"]["Exec"] != "yssbi %f"
            or entry["Desktop Entry"]["Type"] != "Application"):
        raise ValueError("Invalid desktop launcher")
    return expected_files | expected_directories


def clean_environment(home):
    environment = dict(os.environ, HOME=str(home))
    for key in ("LD_LIBRARY_PATH", "YSSBI_APP_DATA_DIR", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"):
        environment.pop(key, None)
    return environment


def verify_help(launcher, home):
    run(launcher, "--help", cwd=home, env=clean_environment(home), timeout=30)


def safe_payload_path(name, absolute=False):
    prefix = "/" if absolute else "./"
    value = name.removeprefix(prefix)
    path = PurePosixPath(value)
    if ((absolute and not name.startswith("/"))
            or not value or path.is_absolute() or path.as_posix() != value
            or any(part in (".", "..") for part in path.parts)
            or any(ord(character) < 32 or ord(character) == 127 for character in value)):
        raise ValueError(f"Unsafe package path: {name!r}")
    return path


def rpm_metadata(asset, version, architecture):
    metadata = capture("rpm", "-qp", "--qf", "%{NAME}\n%{VERSION}\n%{RELEASE}\n%{ARCH}\n%{OS}\n", asset).splitlines()
    if metadata != ["yssbi", version, "1", ARCHITECTURES[architecture]["rpm"], "linux"]:
        raise ValueError(f"Unexpected RPM metadata: {metadata}")
    rows = capture("rpm", "-qp", "--qf",
                   "[%{FILENAMES}\t%{FILEMODES}\t%{FILEUSERNAME}\t%{FILEGROUPNAME}\t%{FILELINKTOS}\t%{FILEFLAGS}\t%{FILESIZES}\n]", asset)
    files = {}
    for row in rows.splitlines():
        name, raw_mode, owner, group, target, raw_flags, raw_size = row.split("\t")
        path = safe_payload_path(name, absolute=True)
        mode, flags, size = int(raw_mode), int(raw_flags), int(raw_size)
        if path in files or owner != "root" or group != "root":
            raise ValueError(f"Invalid RPM ownership or duplicate path: {name}")
        if path == LAUNCHER:
            expected_mode = stat.S_IFLNK | 0o777
            if target != LINK_TARGET:
                raise ValueError("Unsafe RPM launcher symlink")
        else:
            if target:
                raise ValueError(f"Unexpected RPM symlink: {name}")
            if stat.S_ISDIR(mode):
                expected_mode = stat.S_IFDIR | 0o755
            else:
                expected_mode = stat.S_IFREG | (0o755 if path == BINARY else 0o644)
        if mode != expected_mode:
            raise ValueError(f"Invalid RPM file type or permissions: {name}")
        # %license (128) may also inherit %doc (2) from /usr/share/doc.
        allowed_flags = (128, 128 | 2) if path == RPM_NOTICE else (0,)
        if flags not in allowed_flags:
            raise ValueError(f"Unexpected RPM file flags (notices must be %license): {name}")
        files[path] = (mode, size)
    if not files or RPM_NOTICE not in files:
        raise ValueError("RPM has no payload or no marked license notices")
    # No package-supplied symlink may be an ancestor of another archive entry.
    if any(LAUNCHER in path.parents for path in files):
        raise ValueError("RPM payload traverses its launcher symlink")
    return files


def validate_archive(archive, files):
    seen = set()
    for entry in archive:
        path = safe_payload_path(entry.name)
        if path in seen or path not in files:
            raise ValueError(f"Duplicate or unlisted archive path: {entry.name}")
        seen.add(path)
        mode, size = files[path]
        if entry.uid or entry.gid or entry.mode != stat.S_IMODE(mode):
            raise ValueError(f"Archive ownership or permissions differ from RPM metadata: {entry.name}")
        if stat.S_ISLNK(mode):
            valid = (entry.issym() and path == LAUNCHER and entry.linkname == LINK_TARGET
                     and entry.size == 0)
        elif stat.S_ISDIR(mode):
            valid = entry.isdir() and entry.size == 0
        else:
            valid = entry.isfile() and not entry.issparse() and entry.size == size
        if not valid:
            raise ValueError(f"Unsafe or mismatched archive entry: {entry.name}")
    if seen != set(files):
        raise ValueError("RPM metadata and archive payload paths differ")


def verify_rpm_contents(asset, version, architecture):
    files = rpm_metadata(asset, version, architecture)
    requirements = set(capture("rpm", "-qp", "--requires", asset).splitlines())
    if not set(RPM_RUNTIME_DEPENDENCIES) <= requirements:
        raise ValueError("RPM lacks required dynamically loaded desktop dependencies")
    with tempfile.TemporaryDirectory(prefix="yssbi-rpm-", dir="/tmp") as temporary:
        work = Path(temporary)
        archive_path = work / "payload.tar"
        with asset.open("rb") as source, archive_path.open("wb") as output:
            run("rpm2archive", "--nocompression", "-", stdin=source, stdout=output)
        root = work / "root"
        root.mkdir()
        with tarfile.open(archive_path) as archive:
            validate_archive(archive, files)
            archive.extractall(root, filter="data")
        expected = verify_payload(root, "rpm", architecture)
        if set(files) != expected:
            raise ValueError(f"RPM owns unexpected or missing files: {set(files) ^ expected}")
        scanner = Path(capture("rpm", "--eval", "%{_rpmconfigdir}").strip()) / "rpmdeps"
        scanned = set(capture(scanner, "--requires", root / BINARY).splitlines())
        if not scanned or not any(dependency.startswith("libc.so.6") for dependency in scanned):
            raise ValueError("RPM ELF scanner did not report libc dependencies")
        if not scanned <= requirements:
            raise ValueError(f"RPM lacks ELF runtime dependencies: {scanned - requirements}")
        home = work / "home"
        home.mkdir()
        verify_help(root / BINARY, home)
    print("RPM checksum, metadata, safe payload, sample hashes, dependencies and native --help passed")


def installation_status(package_format):
    if package_format == "deb":
        result = subprocess.run(["dpkg-query", "-W", "-f=${db:Status-Status}\\n", "yssbi"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if result.returncode not in (0, 1):
            raise RuntimeError(f"Cannot inspect dpkg installation: {result.stderr}")
        return result.stdout.strip() or "not-installed"
    result = subprocess.run(["rpm", "-q", "yssbi"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if result.returncode not in (0, 1) or result.stderr.strip():
        raise RuntimeError(f"Cannot inspect RPM installation: {result.stderr}")
    return "installed" if result.returncode == 0 else "not-installed"


def refuse_existing_install(package_format):
    if installation_status(package_format) != "not-installed":
        raise ValueError("Refusing to modify an existing yssbi installation or residual configuration")
    for path in installed_paths():
        if path.exists() or path.is_symlink():
            raise ValueError(f"Refusing to overwrite a pre-existing installation path: {path}")


def installed_paths():
    return tuple(Path("/") / path for path in (BINARY.parent, LAUNCHER, DESKTOP, RPM_NOTICE.parent))


def verify_deb_metadata(asset, version, architecture):
    for field, expected in (("Package", "yssbi"), ("Version", version + "-1"),
                            ("Architecture", ARCHITECTURES[architecture]["deb"])):
        if capture("dpkg-deb", "--field", asset, field).strip() != expected:
            raise ValueError(f"Unexpected DEB {field}")
    depends = capture("dpkg-deb", "--field", asset, "Depends")
    names = {clause.strip().split()[0] for clause in depends.split(",") if clause.strip()}
    if not set(DEB_RUNTIME_DEPENDENCIES) | {"libc6"} <= names:
        raise ValueError("DEB lacks ELF or dynamically loaded desktop dependencies")


def install_smoke(asset, package_format, version, architecture):
    refuse_existing_install(package_format)
    if package_format == "deb":
        verify_deb_metadata(asset, version, architecture)
        os.environ["DEBIAN_FRONTEND"] = "noninteractive"
        run("apt-get", "update")
    else:
        verify_rpm_contents(asset, version, architecture)
    manager = "apt-get" if package_format == "deb" else "dnf"
    run(manager, "install", "-y", asset)
    # These are the real application data paths, not package-owned fixture files.
    with tempfile.TemporaryDirectory(prefix="yssbi-user-", dir="/home") as temporary:
        home = Path(temporary)
        sentinels = [home / suffix for suffix in (
            ".local/share/com.zjy.yssbi/settings.json",
            ".local/share/com.zjy.yssbi/projects/keep.yssbi",
            ".config/com.zjy.yssbi/settings.json",
            "Documents/keep.yssbi",
        )]
        for sentinel in sentinels:
            sentinel.parent.mkdir(parents=True, exist_ok=True)
            sentinel.write_text("preserve-user-data\n", encoding="utf-8")
        operations = ("remove", "purge") if package_format == "deb" else ("remove",)
        for operation in operations:
            if operation == "purge":
                # Reinstall so purge exercises a complete installation too.
                run(manager, "install", "-y", asset)
            paths = verify_payload(Path("/"), package_format, architecture)
            for relative in paths:
                info = (Path("/") / relative).lstat()
                if info.st_uid or info.st_gid:
                    raise ValueError(f"Installed package path is not root-owned: {relative}")
            notice = RPM_NOTICE if package_format == "rpm" else RPM_NOTICE.with_name("copyright")
            owned = [str(Path("/") / path) for path in (BINARY, LAUNCHER, DESKTOP, notice)]
            if package_format == "deb":
                run("dpkg-query", "-W", "-f=${Package} ${Version} ${Architecture} ${Status}\\n", "yssbi")
                run("dpkg-query", "-S", *owned)
            else:
                run("rpm", "-q", "--qf", "%{NAME} %{VERSION}-%{RELEASE} %{ARCH}\\n", "yssbi")
                run("rpm", "-qf", *owned)
            verify_help(Path("/") / LAUNCHER, home)
            run(manager, operation, "-y", "yssbi")
            allowed_status = {"not-installed"}
            if package_format == "deb" and operation == "remove":
                allowed_status.add("config-files")
            if installation_status(package_format) not in allowed_status:
                raise ValueError("Package database still reports an installation after removal")
            if any(path.exists() or path.is_symlink() for path in installed_paths()):
                raise ValueError("Package-owned installation files remain after removal")
            if any(path.read_text(encoding="utf-8") != "preserve-user-data\n" for path in sentinels):
                raise ValueError("Uninstall changed user data")
    print("Native install, remove/purge where supported, payload and user-data preservation passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("format", choices=("deb", "rpm", "rpm-contents"))
    parser.add_argument("asset", type=Path)
    args = parser.parse_args()
    baseline = platform.freedesktop_os_release()
    host = (baseline.get("ID"), baseline.get("VERSION_ID"))
    if args.format == "rpm-contents":
        if host not in (("ubuntu", "24.04"), ("fedora", "44")):
            parser.error("RPM content inspection requires Ubuntu 24.04 or Fedora 44")
    else:
        expected = ("ubuntu", "24.04") if args.format == "deb" else ("fedora", "44")
        if host != expected or os.geteuid() != 0:
            parser.error("Installation smoke requires root on an ephemeral clean " + " ".join(expected) + " host")
    architecture = platform.machine()
    if architecture not in ARCHITECTURES:
        parser.error(f"Unsupported native host architecture: {architecture}")
    asset = args.asset.resolve(strict=True)
    if asset.with_name(asset.name + ".sha256").read_text(encoding="utf-8").strip() != f"{sha256(asset)}  {asset.name}":
        raise ValueError("Package checksum mismatch")
    package_format = "deb" if args.format == "deb" else "rpm"
    version = asset_version(asset, package_format, architecture)
    if args.format == "rpm-contents":
        verify_rpm_contents(asset, version, architecture)
    else:
        install_smoke(asset, package_format, version, architecture)


if __name__ == "__main__":
    main()
