# UniFFI uses JNA through reflection
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.procastinatioff_core.** { *; }
-dontwarn java.awt.**
