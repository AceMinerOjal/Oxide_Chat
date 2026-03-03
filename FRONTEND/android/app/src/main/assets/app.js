import { initializeApp } from "https://www.gstatic.com/firebasejs/10.8.0/firebase-app.js";
import {
  getAuth,
  signInWithPopup,
  signInWithRedirect,
  getRedirectResult,
  GoogleAuthProvider,
  GithubAuthProvider,
  onAuthStateChanged,
  signOut,
} from "https://www.gstatic.com/firebasejs/10.8.0/firebase-auth.js";
import { getFirebaseConfig } from "./assets/js/firebase-config.js";

const authPanelEl = document.getElementById("authPanel");
const chatShellEl = document.getElementById("chatShell");
const googleBtn = document.getElementById("googleBtn");
const githubBtn = document.getElementById("githubBtn");
const logoutBtn = document.getElementById("logoutBtn");
const userChipEl = document.getElementById("userChip");
const toastEl = document.getElementById("loginToast");
const statusEl = document.getElementById("status");
const messagesEl = document.getElementById("messages");
const connectBtn = document.getElementById("connectBtn");
const disconnectBtn = document.getElementById("disconnectBtn");
const baseUrlEl = document.getElementById("baseUrl");
const roomIdEl = document.getElementById("roomId");
const composer = document.getElementById("composer");
const messageInputEl = document.getElementById("messageInput");

const BASE_URL_STORAGE_KEY = "oxide.baseUrl";
const ROOM_ID_STORAGE_KEY = "oxide.roomId";

const firebaseConfig = getFirebaseConfig();
const app = initializeApp(firebaseConfig);
const auth = getAuth(app);
let currentUser = null;
let socket = null;
const isAndroidWebView = /\bAndroid\b/i.test(globalThis.navigator?.userAgent || "");

if (baseUrlEl) {
  const savedBaseUrl = globalThis.localStorage?.getItem(BASE_URL_STORAGE_KEY);
  baseUrlEl.value = savedBaseUrl || "";
}

if (roomIdEl) {
  const savedRoomId = globalThis.localStorage?.getItem(ROOM_ID_STORAGE_KEY);
  if (savedRoomId) {
    roomIdEl.value = savedRoomId;
  }
}

baseUrlEl?.addEventListener("change", () => {
  globalThis.localStorage?.setItem(BASE_URL_STORAGE_KEY, baseUrlEl.value.trim());
});

roomIdEl?.addEventListener("change", () => {
  globalThis.localStorage?.setItem(ROOM_ID_STORAGE_KEY, roomIdEl.value.trim());
});

const showToast = (message) => {
  toastEl.textContent = message;
  toastEl.classList.add("show");
  setTimeout(() => toastEl.classList.remove("show"), 3000);
};

function senderIdFor(user) {
  if (!user) {
    throw new Error("Authentication required");
  }
  return user.displayName?.trim() || `user-${user.uid.slice(0, 8)}`;
}

function setStatus(text, color = "#cdd6f4") {
  statusEl.textContent = text;
  statusEl.style.color = color;
}

function appendMessage({ sender_id, payload_cipher, created_at }, isLocal = false) {
  const li = document.createElement("li");
  li.className = `message ${isLocal ? "me" : ""}`;

  const meta = document.createElement("p");
  meta.className = "meta";
  const ts = new Date(created_at).toLocaleTimeString();
  meta.textContent = `${sender_id} • ${ts}`;

  const body = document.createElement("p");
  body.className = "body";
  body.textContent = decodeUtf8(payload_cipher) ?? toHex(payload_cipher);

  li.append(meta, body);
  messagesEl.appendChild(li);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

function toCipherBytes(plaintext) {
  return Array.from(new TextEncoder().encode(plaintext));
}

function decodeUtf8(bytes) {
  try {
    return new TextDecoder().decode(new Uint8Array(bytes));
  } catch {
    return null;
  }
}

function toHex(bytes) {
  return bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function parseIncomingMessageData(data) {
  if (typeof data === "string") {
    return JSON.parse(data);
  }
  if (data instanceof ArrayBuffer) {
    return JSON.parse(new TextDecoder().decode(new Uint8Array(data)));
  }
  if (data instanceof Blob) {
    const text = await data.text();
    return JSON.parse(text);
  }
  throw new Error("unsupported websocket frame type");
}

function buildRoomSocketUrl() {
  const rawBase = baseUrlEl.value.trim();
  const roomId = roomIdEl.value.trim();
  if (!rawBase || !roomId) {
    throw new Error("Base URL and room ID are required");
  }

  if (!rawBase.startsWith("ws://") && !rawBase.startsWith("wss://")) {
    throw new Error("Base URL must start with ws:// or wss://");
  }

  const url = new URL(rawBase);
  const isLocalHost = url.hostname === "localhost" || url.hostname === "127.0.0.1";
  const isAndroidWebView = /\bAndroid\b/i.test(globalThis.navigator?.userAgent || "");

  // Android emulator cannot reach host localhost directly.
  if (isLocalHost && isAndroidWebView) {
    url.hostname = "10.0.2.2";
  }

  const base = url.toString().replace(/\/$/, "");
  return `${base}/room/${encodeURIComponent(roomId)}`;
}

function setConnectedState(connected) {
  connectBtn.disabled = connected;
  disconnectBtn.disabled = !connected;
  messageInputEl.disabled = !connected;
}

function setAuthUi(user) {
  if (user) {
    authPanelEl.classList.add("hidden");
    chatShellEl.classList.remove("hidden");
    userChipEl.textContent = `Signed in as ${user.displayName || user.email || user.uid}`;
    setStatus(`Ready. Sender ID: ${senderIdFor(user)}`);
    return;
  }

  authPanelEl.classList.remove("hidden");
  chatShellEl.classList.add("hidden");
}

function connect() {
  if (!currentUser) {
    throw new Error("Please sign in first");
  }
  if (socket && socket.readyState === WebSocket.OPEN) {
    return;
  }

  const wsUrl = buildRoomSocketUrl();
  const senderId = senderIdFor(currentUser);
  socket = new WebSocket(wsUrl);
  socket.binaryType = "arraybuffer";

  setStatus(`Connecting to ${wsUrl}...`, "#89b4fa");

  socket.onopen = () => {
    setConnectedState(true);
    setStatus(`Connected as ${senderId}`, "#a6e3a1");
  };

  socket.onmessage = async (event) => {
    try {
      const incoming = await parseIncomingMessageData(event.data);
      if (incoming.sender_id && Array.isArray(incoming.payload_cipher)) {
        appendMessage(incoming, incoming.sender_id === senderId);
      }
    } catch {
      setStatus("Received non-JSON message from server", "#f38ba8");
    }
  };

  socket.onerror = () => {
    setStatus(`WebSocket error at ${wsUrl}`, "#f38ba8");
  };

  socket.onclose = (event) => {
    setConnectedState(false);
    const reason = event.reason ? ` (${event.reason})` : "";
    setStatus(`Disconnected [${event.code}]${reason}`, "#cdd6f4");
    socket = null;
  };
}

function disconnect() {
  if (socket) {
    socket.close(1000, "Client closed connection");
  }
}

function sendMessage(text) {
  if (!currentUser) {
    throw new Error("Please sign in first");
  }
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    throw new Error("No active WebSocket connection");
  }

  const senderId = senderIdFor(currentUser);
  const message = {
    sender_id: senderId,
    payload_cipher: toCipherBytes(text),
    created_at: Date.now(),
  };

  socket.send(JSON.stringify(message));
  appendMessage(message, true);
}

const login = async (ProviderClass) => {
  try {
    const provider = new ProviderClass();
    provider.setCustomParameters({ prompt: "select_account" });

    // Firebase popups are often blocked or unsupported inside Android WebView.
    if (isAndroidWebView) {
      showToast("Opening sign-in...");
      setStatus("Redirecting to provider login...", "#89b4fa");
      await signInWithRedirect(auth, provider);
      return;
    }

    await signInWithPopup(auth, provider);
  } catch (err) {
    console.error("Auth error:", err?.code || "unknown");
    const code = err?.code || "unknown";
    showToast(`Authentication failed (${code})`);
    setStatus(`Authentication failed: ${code}`, "#f38ba8");
  }
};

googleBtn.onclick = () => login(GoogleAuthProvider);
githubBtn.onclick = () => login(GithubAuthProvider);

logoutBtn.onclick = async () => {
  try {
    disconnect();
    await signOut(auth);
    showToast("Signed out");
  } catch (err) {
    console.error("Logout error:", err?.code || "unknown");
    showToast("Sign out failed. Please try again.");
  }
};

onAuthStateChanged(auth, (user) => {
  currentUser = user;
  if (user) {
    showToast(`Welcome back, ${user.displayName || "friend"}`);
    setAuthUi(user);
    return;
  }

  disconnect();
  setConnectedState(false);
  setAuthUi(null);
});

if (isAndroidWebView) {
  getRedirectResult(auth).catch((err) => {
    console.error("Redirect auth error:", err?.code || "unknown");
    const code = err?.code || "unknown";
    showToast(`Redirect auth failed (${code})`);
    setStatus(`Redirect auth failed: ${code}`, "#f38ba8");
  });
}

connectBtn.addEventListener("click", () => {
  try {
    connect();
  } catch (error) {
    setStatus(error.message, "#f38ba8");
  }
});

disconnectBtn.addEventListener("click", () => {
  disconnect();
});

composer.addEventListener("submit", (event) => {
  event.preventDefault();
  const text = messageInputEl.value.trim();
  if (!text) {
    return;
  }

  try {
    sendMessage(text);
    messageInputEl.value = "";
  } catch (error) {
    setStatus(error.message, "#f38ba8");
  }
});

setConnectedState(false);
setStatus("Please sign in to continue");
