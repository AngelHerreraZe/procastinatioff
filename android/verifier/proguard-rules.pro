# UniFFI uses JNA through reflection
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.procastinatioff_verify.** { *; }
-dontwarn java.awt.**
# MediaPipe Tasks uses protobuf through reflection
-keep class com.google.mediapipe.** { *; }
-dontwarn com.google.mediapipe.**
