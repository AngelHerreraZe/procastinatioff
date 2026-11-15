# UniFFI usa JNA por reflexión
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.procastinacioff_verify.** { *; }
-dontwarn java.awt.**
# MediaPipe Tasks usa protobuf por reflexión
-keep class com.google.mediapipe.** { *; }
-dontwarn com.google.mediapipe.**
