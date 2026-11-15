#!/usr/bin/env python3
"""Falla si un APK pide permisos fuera de su lista aprobada (incluye los que añaden librerías).

Uso: python scripts/check_android_permissions.py <aapt2> <apk> [--verifier]
     --verifier: APK de cámara/micrófono, que NUNCA puede tener red.
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
    "android.permission.FOREGROUND_SERVICE",            # WorkManager (trabajos largos) y EnforcerService
    "android.permission.FOREGROUND_SERVICE_SYSTEM_EXEMPTED",  # modo familiar: suspensión de apps (Device Owner)
    "android.permission.USE_BIOMETRIC",                 # modo familiar: el tutor confirma con huella o PIN
    "android.permission.USE_FINGERPRINT",               # (androidx.biometric en Android antiguos)
    "com.google.android.c2dm.permission.RECEIVE",       # FCM
    "dev.procastinacioff.permission.VERIFY",            # abrir el verificador (permiso de firma)
    "dev.procastinacioff.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
}
VERIFIER = {
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
    "dev.procastinacioff.permission.VERIFY",            # lo declara (signature) para que solo lo abra la app
    "dev.procastinacioff.verifier.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
}
NETWORK = {"android.permission.INTERNET", "android.permission.ACCESS_NETWORK_STATE",
           "android.permission.ACCESS_WIFI_STATE", "android.permission.CHANGE_NETWORK_STATE"}

aapt2, apk = sys.argv[1], sys.argv[2]
verifier = "--verifier" in sys.argv
allowed = VERIFIER if verifier else APP
out = subprocess.run([aapt2, "dump", "permissions", apk], check=True, capture_output=True, text=True).stdout
found = set(re.findall(r"name='([^']+)'", out))
if verifier and found & NETWORK:
    print("🔒 El verificador tiene permisos de RED:", *sorted(found & NETWORK), sep="\n  ")
    sys.exit(1)
extra = sorted(found - allowed)
if extra:
    print("Permisos no aprobados:", *extra, sep="\n  ")
    sys.exit(1)
print(f"Permisos del APK{' verificador' if verifier else ''}: OK ({len(found)})")
