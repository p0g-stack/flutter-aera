#!/usr/bin/env python3
"""Assemble the runtime kit: the expanded payload of spec/aerap.md minus the
app (`flutter_assets`, `libapp.so`).

Every library goes in usr/lib as a real file under its soname (the payload
format has no symlinks), including the glibc loader that aera-plugin execs.
The DT_NEEDED closure of every ELF is resolved from --sysroot directories.

    ci/assemble-kit.py --out kit --embedder aera-flutter --launcher aera-plugin \\
        --engine libflutter_engine.so --icu icudtl.dat --mesa mesa-dest \\
        --sysroot /usr/lib/aarch64-linux-gnu --sysroot /lib/aarch64-linux-gnu \\
        --ca-bundle /etc/ssl/certs/ca-certificates.crt --pins kit.json \\
        --alsa-module libasound_module_pcm_aera.so --alsa-conf audio/aera.conf
"""
import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

LOADERS = {"arm64": "ld-linux-aarch64.so.1", "x64": "ld-linux-x86-64.so.2"}
# dlopen()ed, so no DT_NEEDED names them: the embedder opens libEGL, Zink
# opens the Vulkan loader, the Vulkan loader opens Turnip (arm64 only), and
# audio plugins (miniaudio) open ALSA.
DLOPENED = ["libEGL.so.1", "libGLESv2.so.2", "libvulkan.so.1", "libasound.so.2"]


def needed(path, readelf):
    out = subprocess.run([readelf, "-d", str(path)], check=True, capture_output=True, text=True).stdout
    return [l.split("[", 1)[1].rstrip("]") for l in out.splitlines() if "(NEEDED)" in l]


def find(name, search):
    for d in search:
        p = Path(d) / name
        if p.exists():
            return p.resolve()
    return None


def is_elf(p):
    with p.open("rb") as f:
        return f.read(4) == b"\x7fELF"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--embedder", type=Path, required=True)
    ap.add_argument("--launcher", type=Path, required=True)
    ap.add_argument("--engine", type=Path, required=True)
    ap.add_argument("--icu", type=Path, required=True)
    ap.add_argument("--mesa", type=Path, required=True, help="Mesa DESTDIR (has usr/lib)")
    ap.add_argument("--sysroot", action="append", default=[])
    ap.add_argument("--ca-bundle", type=Path, required=True)
    ap.add_argument("--pins", type=Path, required=True, help="kit.json to place at the kit root")
    ap.add_argument("--alsa-module", type=Path, required=True, help="libasound_module_pcm_aera.so (audio/pcm_aera.c)")
    ap.add_argument("--alsa-conf", type=Path, required=True, help="audio/aera.conf")
    ap.add_argument("--readelf", default="readelf")
    ap.add_argument("--arch", choices=sorted(LOADERS), default="arm64")
    a = ap.parse_args()

    out = a.out
    if out.exists() and any(out.iterdir()):
        sys.exit(f"{out} is not empty")
    lib = out / "usr/lib"
    dirs = (lib, lib / "alsa-lib", out / "usr/bin", out / "usr/share/flutter", out / "usr/share/alsa", out / "usr/share/vulkan/icd.d",
            out / "etc/ssl/certs")
    for d in dirs:
        d.mkdir(parents=True, exist_ok=True)

    shutil.copy2(a.embedder, out / "usr/bin/aera-flutter")
    shutil.copy2(a.launcher, out / "usr/bin/aera-plugin")
    shutil.copy2(Path(__file__).resolve().parent.parent / "launcher/xdg-user-dir", out / "usr/bin/xdg-user-dir")
    for b in ("aera-flutter", "aera-plugin", "xdg-user-dir"):
        (out / "usr/bin" / b).chmod(0o755)
    shutil.copy2(a.engine, lib / "libflutter_engine.so")
    shutil.copy2(a.icu, out / "usr/share/flutter/icudtl.dat")
    shutil.copyfile(a.ca_bundle, out / "etc/ssl/certs/ca-certificates.crt")
    # ALSA reads only these (src/env.rs): the default device is AERA's audio
    # bridge (src/audio.rs).
    shutil.copy2(a.alsa_module, lib / "alsa-lib/libasound_module_pcm_aera.so")
    shutil.copyfile(a.alsa_conf, out / "usr/share/alsa/alsa.conf")

    mesa_lib = a.mesa / "usr/lib"
    search = [mesa_lib] + a.sysroot
    for g in mesa_lib.glob("libgallium-*.so"):
        shutil.copy2(g, lib / g.name)
    for name in DLOPENED + [LOADERS[a.arch]]:
        src = find(name, search)
        if src is None:
            sys.exit(f"missing {name}")
        shutil.copy2(src, lib / name)
    # The Vulkan ICDs (Turnip on arm64, gfxstream_vk on x64), pointing at the
    # library next to them in the payload; the launcher names them in
    # VK_DRIVER_FILES.
    names = []
    for src in sorted((a.mesa / "usr/share/vulkan/icd.d").glob("*_icd*.json")):
        icd = json.loads(src.read_text())
        so = Path(icd["ICD"]["library_path"]).name
        shutil.copy2(mesa_lib / so, lib / so)
        icd["ICD"]["library_path"] = f"../../../lib/{so}"
        name = src.name.split("_icd")[0]
        (out / f"usr/share/vulkan/icd.d/{name}_icd.json").write_text(json.dumps(icd, indent=2) + "\n")
        names.append(name)
    want = {"arm64": "freedreno", "x64": "gfxstream"}[a.arch]
    if not any(n.startswith(want) for n in names):
        sys.exit(f"{a.arch} kit without the {want} Vulkan driver")
    if (a.mesa / "usr/share/drirc.d").is_dir():
        shutil.copytree(a.mesa / "usr/share/drirc.d", out / "usr/share/drirc.d")

    pending = [p for p in out.rglob("*") if p.is_file() and is_elf(p)]
    seen = set()
    while pending:
        path = pending.pop()
        for name in needed(path, a.readelf):
            if name in seen:
                continue
            seen.add(name)
            target = lib / name
            if target.exists():
                continue
            src = find(name, search)
            if src is None:
                sys.exit(f"{path.name} needs {name}, which no sysroot provides")
            shutil.copy2(src, target)
            pending.append(target)

    shutil.copy2(a.pins, out / "kit.json")
    files = [p for p in out.rglob("*") if p.is_file()]
    print(f"kit: {len(files)} files, {sum(p.stat().st_size for p in files) / 1e6:.1f} MB in {out}")


if __name__ == "__main__":
    main()
