const fallbackFirebaseConfig = Object.freeze({
  apiKey: "YOUR_FIREBASE_API_KEY",
  authDomain: "YOUR_FIREBASE_AUTH_DOMAIN",
  projectId: "YOUR_FIREBASE_PROJECT_ID",
  storageBucket: "YOUR_FIREBASE_STORAGE_BUCKET",
  messagingSenderId: "YOUR_FIREBASE_MESSAGING_SENDER_ID",
  appId: "YOUR_FIREBASE_APP_ID",
});

const requiredKeys = Object.freeze([
  "apiKey",
  "authDomain",
  "projectId",
  "storageBucket",
  "messagingSenderId",
  "appId",
]);

export const getFirebaseConfig = () => {
  const runtimeConfig = globalThis.__FIREBASE_CONFIG__;
  const candidate =
    runtimeConfig && typeof runtimeConfig === "object"
      ? runtimeConfig
      : fallbackFirebaseConfig;

  for (const key of requiredKeys) {
    if (typeof candidate[key] !== "string" || candidate[key].trim() === "") {
      throw new Error(`Invalid Firebase config: missing ${key}`);
    }
  }

  return Object.freeze({
    apiKey: candidate.apiKey.trim(),
    authDomain: candidate.authDomain.trim(),
    projectId: candidate.projectId.trim(),
    storageBucket: candidate.storageBucket.trim(),
    messagingSenderId: candidate.messagingSenderId.trim(),
    appId: candidate.appId.trim(),
  });
};
