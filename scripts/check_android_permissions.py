#!/usr/bin/env python3
"""Fails if an APK requests permissions outside its approved list (including those added by libraries).

Usage: python scripts/check_android_permissions.py <aapt2> <apk> [--verifier]
       --verifier: the camera/microphone APK, which can NEVER have network access.
"""
import re
import subprocess
import sys

APP = {
    "android.permission.INTERNET",
    "android.permission.ACCESS_NOTIFICATION_POLICY",
    "android.permission.POST_NOTIFICATIONS",
    "android.permission.CAMERA",
    "android.permission.QUERY_ALL_PACKAGES",
    "android.permission.WAKE_LOCK",
    "android.permission.ACCESS_NETWORK_STATE",
    "android.permission.RECEIVE_BOOT_COMPLETED",       # WorkManager
    "android.permission.FOREGROUND_SERVICE",            # WorkManager (long jobs) and EnforcerService
    "android.permission.FOREGROUND_SERVICE_SYSTEM_EXEMPTED",  # family mode: app suspension (Device Owner)
    "android.permission.USE_BIOMETRIC",                 # family mode: the guardian confirms with fingerprint or PIN
    "android.permission.USE_FINGERPRINT",               # (androidx.biometric on older Android)
    "dev.procastinatioff.permission.VERIFY",            # open the verifier (signature permission)
    "android.permission.REQUEST_INSTALL_PACKAGES",      # install the verifier embedded in the app
    "android.permission.health.READ_EXERCISE",          # "Workout" goals: today's exercise minutes (read only)
    "com.google.android.apps.aicore.service.BIND_SERVICE",  # ML Kit GenAI: binds to AICore (Gemini Nano runs on the phone)
    "dev.procastinatioff.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
}
VERIFIER = {
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
    "dev.procastinatioff.permission.VERIFY",            # declared (signature) so only the app can open it
    "dev.procastinatioff.verifier.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
}
NETWORK = {"android.permission.INTERNET", "android.permission.ACCESS_NETWORK_STATE",
           "android.permission.ACCESS_WIFI_STATE", "android.permission.CHANGE_NETWORK_STATE"}

aapt2, apk = sys.argv[1], sys.argv[2]
verifier = "--verifier" in sys.argv
allowed = VERIFIER if verifier else APP
out = subprocess.run([aapt2, "dump", "permissions", apk], check=True, capture_output=True, text=True).stdout
found = set(re.findall(r"name='([^']+)'", out))
if verifier and found & NETWORK:
    print("SECURITY: The verifier has NETWORK permissions:", *sorted(found & NETWORK), sep="\n  ")
    sys.exit(1)
extra = sorted(found - allowed)
if extra:
    print("Permissions not approved:", *extra, sep="\n  ")
    sys.exit(1)
print(f"{'Verifier APK' if verifier else 'APK'} permissions: OK ({len(found)})")
