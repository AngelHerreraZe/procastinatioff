# UniFFI usa JNA por reflexión
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.procastinacioff_core.** { *; }
-dontwarn java.awt.**
