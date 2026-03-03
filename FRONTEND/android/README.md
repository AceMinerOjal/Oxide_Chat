# Oxide Chat Android APK

This folder wraps the `FRONTEND` web app in an Android WebView app.

## Build

Android Studio:
1. Open Android Studio.
2. Choose **Open** and select `FRONTEND/android`.
3. Let Gradle sync.
4. Build a debug APK via **Build > Build Bundle(s) / APK(s) > Build APK(s)**.

CLI:

```bash
cd FRONTEND/android
./gradlew :app:assembleDebug
```

Debug APK:
`FRONTEND/android/app/build/outputs/apk/debug/app-debug.apk`

## Notes
- The bundled web assets are under `app/src/main/assets/`.
- Firebase Google/GitHub sign-in is required before chat connect.
- Android auth uses Firebase redirect flow in WebView.
- Fill in Firebase placeholders before building:
  - `FRONTEND/assets/js/firebase-config.js`
  - `FRONTEND/android/app/src/main/assets/assets/js/firebase-config.js`
- WebSocket endpoint has no hardcoded default in the UI.
- The endpoint is user-provided and persisted between launches.
- For Android emulator + local backend, use:
  `ws://10.0.2.2:<port>`
