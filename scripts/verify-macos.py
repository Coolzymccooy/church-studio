"""Check the packaged universal app, privacy declarations and native launch."""
import json
import pathlib
import plistlib
import subprocess
import sys
import time

app = pathlib.Path(sys.argv[1]).resolve()
with (app / "Contents/Info.plist").open("rb") as stream:
    info = plistlib.load(stream)
assert info.get("NSMicrophoneUsageDescription"), "Missing microphone permission description"
assert info.get("NSLocalNetworkUsageDescription"), "Missing local-network permission description"
binary = app / "Contents/MacOS" / info["CFBundleExecutable"]
architectures = subprocess.check_output(["lipo", "-archs", str(binary)], text=True).strip()
assert "arm64" in architectures and "x86_64" in architectures, architectures
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
process = subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    time.sleep(10)
    assert process.poll() is None, f"Native app exited during startup: {process.returncode}"
finally:
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=20)
report = {
    "version": info["CFBundleShortVersionString"], "architectures": architectures,
    "microphone_description": True, "local_network_description": True,
    "native_launch_10_seconds": True,
    "hardware_audio_and_network_devices": "requires operator testing",
    "notarization": "not verified; this is a test package",
}
pathlib.Path("macos-verification.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
