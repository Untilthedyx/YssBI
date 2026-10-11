import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


packaging = load("package")
macos = load("package_macos")


class PackagingTests(unittest.TestCase):
    def test_wrong_architecture_is_rejected_before_distribution(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "application"
            header = bytearray(20)
            header[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<H", header, 18, 183)
            binary.write_bytes(header)
            packaging.verify_elf(binary, "aarch64")
            with self.assertRaisesRegex(ValueError, "architecture"):
                packaging.verify_elf(binary, "x86_64")
            binary.write_bytes(b"not an executable")
            with self.assertRaisesRegex(ValueError, "ELF"):
                packaging.verify_elf(binary, "aarch64")

    def test_corrupt_sample_payload_blocks_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            samples = Path(temporary)
            payload = samples / "iris/v1/data.parquet"
            payload.parent.mkdir(parents=True)
            original = b"PAR1sample fixturePAR1"
            payload.write_bytes(original)
            catalog = {
                "formatVersion": 1,
                "datasets": [{
                    "id": "iris", "version": 1, "byteSize": len(original),
                    "sha256": hashlib.sha256(original).hexdigest(),
                }],
            }
            (samples / "catalog.json").write_text(json.dumps(catalog))
            (samples / "README.md").write_text("Dataset attribution")
            (samples / "sources.json").write_text("{}")
            packaging.verify_samples(samples)
            payload.write_bytes(b"X" * len(original))
            with self.assertRaisesRegex(ValueError, "does not match"):
                packaging.verify_samples(samples)

    def test_sample_identity_cannot_escape_resource_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            samples = Path(temporary)
            (samples / "catalog.json").write_text(json.dumps({
                "formatVersion": 1,
                "datasets": [{"id": "../outside", "version": 1}],
            }))
            with self.assertRaisesRegex(ValueError, "identity"):
                packaging.verify_samples(samples)

    def test_prerelease_versions_have_valid_apple_numeric_version(self):
        self.assertEqual(macos.numeric_version("0.4.0-rc.2+build.15"), "0.4.0")
        for invalid in ("0.4", "01.4.0", "0.4.0-01", "0.100.0", "10000.0.0"):
            with self.subTest(version=invalid):
                with self.assertRaises(ValueError):
                    macos.numeric_version(invalid)


if __name__ == "__main__":
    unittest.main()
