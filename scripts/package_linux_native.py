#!/usr/bin/env python3
"""Build Ubuntu 24.04 DEBs and GNU/Linux RPMs from verified native inputs.

Asset names retain SemVer. Package versions omit build metadata and use '~'
for prereleases, with dpkg/rpm-native ordering (not arbitrary SemVer collation).
RPM additionally maps prerelease hyphens to dots because Version forbids '-'.
"""

from datetime import UTC, datetime
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import tempfile

from package import verify_elf


DISTRIBUTIONS = {
    "deb": (("ubuntu", "24.04"),),
    "rpm": (("ubuntu", "24.04"), ("fedora", "44")),
}
ARCHITECTURES = {
    "x86_64": {"deb": "amd64", "rpm": "x86_64"},
    "aarch64": {"deb": "arm64", "rpm": "aarch64"},
}
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
# GPUI's wayland-sys, fontconfig, ash and EGL loaders bypass ELF DT_NEEDED;
# ashpd also needs the desktop portal service and a desktop-specific backend.
DEB_RUNTIME_DEPENDENCIES = (
    "libxkbcommon0", "libxkbcommon-x11-0", "libfontconfig1",
    "libwayland-client0", "libwayland-cursor0", "libwayland-egl1",
    "libegl1", "libvulkan1", "xdg-desktop-portal",
)
RPM_RUNTIME_DEPENDENCIES = (
    "libxkbcommon.so.0()(64bit)", "libxkbcommon-x11.so.0()(64bit)",
    "libfontconfig.so.1()(64bit)", "libwayland-client.so.0()(64bit)",
    "libwayland-cursor.so.0()(64bit)", "libwayland-egl.so.1()(64bit)",
    "libEGL.so.1()(64bit)", "libvulkan.so.1()(64bit)", "xdg-desktop-portal",
)
DESKTOP_ENTRY = (
    "[Desktop Entry]\nType=Application\nName=YssBI\n"
    "Comment=Native data analysis workbench\nExec=yssbi %f\n"
    "Icon=applications-office\nTerminal=false\nCategories=Office;Science;\n"
    "StartupWMClass=com.zjy.yssbi\n"
)


def _validate_format(package_format):
    if package_format not in DISTRIBUTIONS:
        raise ValueError(f"Unsupported native package format: {package_format}")


def package_version(version, package_format):
    """Return a validated native upstream version, without package revision."""
    _validate_format(package_format)
    match = SEMVER.fullmatch(version)
    if not match:
        raise ValueError(f"Invalid release version: {version}")
    major, minor, patch, prerelease, _metadata = match.groups()
    if prerelease and any(
        identifier.isdigit() and len(identifier) > 1 and identifier.startswith("0")
        for identifier in prerelease.split(".")
    ):
        raise ValueError(f"Invalid release version: {version}")
    result = f"{major}.{minor}.{patch}"
    if prerelease:
        if package_format == "rpm":
            prerelease = prerelease.replace("-", ".")
        result += "~" + prerelease
    return result


def native_asset_name(version, architecture, package_format):
    package_version(version, package_format)
    if architecture not in ARCHITECTURES:
        raise ValueError(f"Unsupported native architecture: {architecture}")
    baseline = "ubuntu24.04" if package_format == "deb" else "linux"
    return f"yssbi-{version}-{baseline}-{architecture}.{package_format}"


def validate_distribution(package_format, os_release=None):
    """Reject non-baseline hosts before invoking any build or packaging tools."""
    _validate_format(package_format)
    if os_release is None:
        os_release = platform.freedesktop_os_release()
    baselines = DISTRIBUTIONS[package_format]
    if (os_release.get("ID"), os_release.get("VERSION_ID")) not in baselines:
        expected = " or ".join(f"{distribution} {version}" for distribution, version in baselines)
        raise ValueError(
            f"{package_format} packaging requires {expected}; "
            f"found {os_release.get('ID', 'unknown')} {os_release.get('VERSION_ID', 'unknown')}"
        )


def _run(arguments, **kwargs):
    print("+", " ".join(map(str, arguments)), flush=True)
    return subprocess.run(list(map(str, arguments)), check=True, **kwargs)


def _copy_data_tree(source, destination):
    if not stat.S_ISDIR(source.lstat().st_mode):
        raise ValueError(f"Sample directory must be a real directory: {source}")
    destination.mkdir(mode=0o755)
    destination.chmod(0o755)
    for child in sorted(source.iterdir()):
        mode = child.lstat().st_mode
        target = destination / child.name
        if stat.S_ISDIR(mode):
            _copy_data_tree(child, target)
        elif stat.S_ISREG(mode):
            if re.search(r"\.so(?:\.|$)", child.name):
                raise ValueError(f"Native packages must not bundle shared libraries: {child}")
            shutil.copyfile(child, target)
            target.chmod(0o644)
        else:
            raise ValueError(f"Sample data must contain only regular files and directories: {child}")


def _stage_payload(binary, samples, notices, root, package_format):
    for directory in (
        "usr/bin", "usr/libexec/yssbi/resources", "usr/share/applications", "usr/share/doc/yssbi",
    ):
        (root / directory).mkdir(parents=True, exist_ok=True)
    packaged_binary = root / "usr/libexec/yssbi/yss-desktop-gpui"
    shutil.copyfile(binary, packaged_binary)
    packaged_binary.chmod(0o755)
    _copy_data_tree(samples, root / "usr/libexec/yssbi/resources/samples")
    # Minimal Ubuntu installations retain copyright files but exclude other docs.
    notice_name = "copyright" if package_format == "deb" else "THIRD_PARTY_NOTICES.txt"
    packaged_notices = root / "usr/share/doc/yssbi" / notice_name
    shutil.copyfile(notices, packaged_notices)
    packaged_notices.chmod(0o644)
    desktop = root / "usr/share/applications/com.zjy.yssbi.desktop"
    desktop.write_text(DESKTOP_ENTRY, encoding="utf-8")
    desktop.chmod(0o644)
    (root / "usr/bin/yssbi").symlink_to("../libexec/yssbi/yss-desktop-gpui")
    # Normalize all ancestors too, independently of the caller's umask.
    root.chmod(0o755)
    for directory, _children, _files in os.walk(root):
        Path(directory).chmod(0o755)
    return packaged_binary


def _build_deb(root, work, version, architecture):
    # dpkg-shlibdeps needs a source control file even for a prebuilt executable.
    debian = work / "debian"
    (debian / "control").write_text(
        "Source: yssbi\nSection: science\nPriority: optional\n"
        "Maintainer: zhouyi207 <zhouyi_207@foxmail.com>\n\n"
        "Package: yssbi\nArchitecture: any\nDescription: Native data analysis workbench\n",
        encoding="utf-8",
    )
    # dpkg-shlibdeps locates the package root by this directory before control exists.
    control = root / "DEBIAN"
    control.mkdir(mode=0o755)
    control.chmod(0o755)
    result = _run(
        ["dpkg-shlibdeps", "-O", "-e" + str(root.relative_to(work) / "usr/libexec/yssbi/yss-desktop-gpui")],
        cwd=work, stdout=subprocess.PIPE, text=True,
    )
    dependencies = [line.removeprefix("shlibs:Depends=") for line in result.stdout.splitlines()
                    if line.startswith("shlibs:Depends=")]
    if len(dependencies) != 1 or not dependencies[0].strip():
        raise ValueError("dpkg-shlibdeps did not report ELF runtime dependencies")
    installed_size = sum(path.stat().st_size for path in root.rglob("*")
                         if path.is_file() and not path.is_symlink())
    (control / "control").write_text(
        "Package: yssbi\n"
        f"Version: {version}-1\nArchitecture: {ARCHITECTURES[architecture]['deb']}\n"
        "Section: science\nPriority: optional\n"
        "Maintainer: zhouyi207 <zhouyi_207@foxmail.com>\n"
        "Homepage: https://github.com/zhouyi207/YssBI\n"
        f"Installed-Size: {(installed_size + 1023) // 1024}\n"
        f"Depends: {dependencies[0]}, {', '.join(DEB_RUNTIME_DEPENDENCIES)}\n"
        "Recommends: xdg-desktop-portal-gtk\n"
        "Description: Native data analysis workbench\n"
        " YssBI desktop application with verified built-in sample datasets.\n",
        encoding="utf-8",
    )
    (control / "control").chmod(0o644)
    result_path = work / "yssbi.deb"
    _run(["dpkg-deb", "--root-owner-group", "--build", root, result_path], cwd=work)
    return result_path


def _build_rpm(root, work, version, architecture):
    topdir = work / "rpmbuild"
    for directory in ("BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS"):
        (topdir / directory).mkdir(parents=True)
    spec = topdir / "SPECS/yssbi.spec"
    epoch = os.environ.get("SOURCE_DATE_EPOCH")
    release_date = datetime.fromtimestamp(int(epoch), UTC) if epoch else datetime.now(UTC)
    spec.write_text(
        # A prebuilt Rust binary has no debuginfo source package. Retain normal
        # BRP checks/strip and ELF Requires, but do not emit private ELF Provides.
        "%global debug_package %{nil}\n"
        "%global _build_id_links none\n"
        "Name: yssbi\n"
        f"Version: {version}\nRelease: 1\n"
        "Summary: Native data analysis workbench\n"
        "License: LicenseRef-Unknown\n"
        "URL: https://github.com/zhouyi207/YssBI\n"
        f"BuildArch: {ARCHITECTURES[architecture]['rpm']}\n"
        "AutoReq: yes\nAutoProv: no\n"
        + "".join(f"Requires: {dependency}\n" for dependency in RPM_RUNTIME_DEPENDENCIES)
        + "Recommends: xdg-desktop-portal-gtk\n\n"
        "%description\n"
        "YssBI desktop application with verified built-in sample datasets.\n"
        "GNU/Linux package; the application license requires maintainer review.\n\n"
        "%prep\n\n%build\n\n%install\n"
        'mkdir -p "%{buildroot}"\n'
        'cp -a -- "$YSSBI_NATIVE_PAYLOAD/." "%{buildroot}/"\n\n'
        "%files\n%defattr(-,root,root,-)\n"
        "/usr/bin/yssbi\n/usr/libexec/yssbi\n"
        "/usr/share/applications/com.zjy.yssbi.desktop\n"
        "%dir /usr/share/doc/yssbi\n"
        # Required legal notices must survive DNF's nodocs installation policy.
        "%license /usr/share/doc/yssbi/THIRD_PARTY_NOTICES.txt\n\n"
        "%changelog\n"
        f"* {release_date:%a %b %d %Y} zhouyi207 <zhouyi_207@foxmail.com> - {version}-1\n"
        "- Package YssBI with built-in sample datasets.\n",
        encoding="utf-8",
    )
    _run(
        ["rpmbuild", "-bb", "--define", f"_topdir {topdir}", "--target", architecture, spec],
        cwd=work, env={**os.environ, "YSSBI_NATIVE_PAYLOAD": str(root)},
    )
    packages = list((topdir / "RPMS").rglob("*.rpm"))
    if len(packages) != 1:
        raise ValueError(f"rpmbuild produced {len(packages)} binary packages; expected exactly one")
    return packages[0]


def package_native_linux(binary, samples, release_version, architecture, output, notices, package_format):
    """Return one complete native asset; never modify inputs or overwrite assets."""
    validate_distribution(package_format)
    name = native_asset_name(release_version, architecture, package_format)
    binary = Path(binary).resolve(strict=True)
    samples = Path(samples).absolute()
    notices = Path(notices).resolve(strict=True)
    verify_elf(binary, architecture)
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    asset = output / name
    if asset.exists() or asset.is_symlink():
        raise ValueError(f"Output already exists: {asset}")
    with tempfile.TemporaryDirectory(prefix="yssbi-native-") as temporary:
        work = Path(temporary).resolve()
        root = work / ("debian/yssbi" if package_format == "deb" else "payload")
        _stage_payload(binary, samples, notices, root, package_format)
        build = {"deb": _build_deb, "rpm": _build_rpm}[package_format]
        completed = build(root, work, package_version(release_version, package_format), architecture)
        if not completed.is_file() or completed.stat().st_size == 0:
            raise ValueError("Native packager produced no artifact")
        # Copy onto the destination filesystem first; link publishes atomically
        # without replacing a pre-existing asset, even with concurrent callers.
        with tempfile.NamedTemporaryFile(prefix=".yssbi-", dir=output) as pending:
            with completed.open("rb") as source:
                shutil.copyfileobj(source, pending)
            pending.flush()
            os.fchmod(pending.fileno(), 0o644)
            os.link(pending.name, asset)
    return asset
