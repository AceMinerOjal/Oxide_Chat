const authPanelEl = document.getElementById("authPanel");
const chatShellEl = document.getElementById("chatShell");
const googleBtn = document.getElementById("googleBtn");
const logoutBtn = document.getElementById("logoutBtn");
const userChipEl = document.getElementById("userChip");
const userEmailEl = document.getElementById("userEmail");
const userUidEl = document.getElementById("userUid");
const profileAvatarEl = document.getElementById("profileAvatar");
const toastEl = document.getElementById("loginToast");
const statusEl = document.getElementById("status");
const messagesEl = document.getElementById("messages");
const connectBtn = document.getElementById("connectBtn");
const disconnectBtn = document.getElementById("disconnectBtn");
const baseUrlEl = document.getElementById("baseUrl");
const roomIdEl = document.getElementById("roomId");
const composer = document.getElementById("composer");
const messageInputEl = document.getElementById("messageInput");

let currentUser = null;
let socket = null;
let connectedRoomId = "";
let reconnectAfterClose = false;
const isAndroidWebView = /\bAndroid\b/i.test(globalThis.navigator?.userAgent || "");
const hasAndroidNativeAuth =
  isAndroidWebView &&
  globalThis.AndroidAuth &&
  typeof globalThis.AndroidAuth.signInWithGoogle === "function";
const BASE_URL_STORAGE_KEY = "oxide.baseUrl";
const ROOM_ID_STORAGE_KEY = "oxide.roomId";
const DEFAULT_BASE_URL = "wss://your-host.example.com";
const STATUS_COLORS = Object.freeze({
  default: "#cdd6f4",
  info: "#89b4fa",
  success: "#a6e3a1",
  warning: "#f9e2af",
  error: "#f38ba8",
});
const PRESENCE_JOIN_KIND = "presence.join";
const PRESENCE_LEAVE_KIND = "presence.leave";
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();
let toastTimerId = null;
const MAX_RENDERED_MESSAGES = 500;
const RECONNECT_BASE_DELAY_MS = 1000;
const RECONNECT_MAX_DELAY_MS = 15000;
const RECONNECT_JITTER_FACTOR = 0.2;
const FIREBASE_APP_MODULE_URL = "https://www.gstatic.com/firebasejs/10.8.0/firebase-app.js";
const FIREBASE_AUTH_MODULE_URL = "https://www.gstatic.com/firebasejs/10.8.0/firebase-auth.js";
const FIREBASE_CONFIG_MODULE_URL = "./assets/js/firebase-config.js";
let authRuntime = null;
let authRuntimePromise = null;
let authStateBound = false;
let redirectResultHandled = false;
let messageFlushHandle = null;
const pendingMessageNodes = [];
let reconnectTimerId = null;
let reconnectAttemptCount = 0;
let manualCloseRequested = false;

function applyStoredValue(element, storageKey, fallbackValue = "") {
  if (!element) {
    return;
  }
  const savedValue = globalThis.localStorage?.getItem(storageKey);
  if (!element.value.trim()) {
    element.value = savedValue || fallbackValue;
    return;
  }
  if (savedValue) {
    element.value = savedValue;
  }
}

function persistOnChange(element, storageKey) {
  element?.addEventListener("change", () => {
    globalThis.localStorage?.setItem(storageKey, element.value.trim());
  });
}

applyStoredValue(baseUrlEl, BASE_URL_STORAGE_KEY, DEFAULT_BASE_URL);
applyStoredValue(roomIdEl, ROOM_ID_STORAGE_KEY);
persistOnChange(baseUrlEl, BASE_URL_STORAGE_KEY);
persistOnChange(roomIdEl, ROOM_ID_STORAGE_KEY);

const showToast = (message) => {
  toastEl.textContent = message;
  toastEl.classList.add("show");
  if (toastTimerId) {
    globalThis.clearTimeout(toastTimerId);
  }
  toastTimerId = globalThis.setTimeout(() => {
    toastEl.classList.remove("show");
    toastTimerId = null;
  }, 3000);
};

function senderIdFor(user) {
  if (!user) {
    throw new Error("Authentication required");
  }
  const uidShort = user.uid.slice(0, 8);
  const label = user.displayName?.trim() || user.email?.trim() || "user";
  return `${label} [${uidShort}]`;
}

function normalizeAndroidUser(payload) {
  if (!payload || typeof payload !== "object") {
    return null;
  }
  if (!payload.uid || typeof payload.uid !== "string") {
    return null;
  }
  return {
    uid: payload.uid,
    displayName: payload.displayName || null,
    email: payload.email || null,
    photoURL: payload.photoURL || null,
  };
}

function readAndroidCurrentUser() {
  if (!hasAndroidNativeAuth || typeof globalThis.AndroidAuth.getCurrentUserJson !== "function") {
    return null;
  }
  try {
    const raw = globalThis.AndroidAuth.getCurrentUserJson();
    if (!raw || raw === "null") {
      return null;
    }
    return normalizeAndroidUser(JSON.parse(raw));
  } catch {
    return null;
  }
}

function setStatus(text, color = STATUS_COLORS.default) {
  statusEl.textContent = text;
  statusEl.style.color = color;
}

function bindAuthStateIfNeeded(runtime) {
  if (authStateBound) {
    return;
  }
  authStateBound = true;

  runtime.firebaseAuth.onAuthStateChanged(runtime.auth, (user) => {
    if (isAndroidWebView) {
      currentUser = user || readAndroidCurrentUser();
      if (!currentUser) {
        disconnect();
      }
      setAuthUi(currentUser);
      return;
    }

    currentUser = user;
    if (user) {
      showToast(`Welcome back, ${user.displayName || "friend"}`);
      setAuthUi(user);
      return;
    }

    disconnect();
    setAuthUi(null);
  });
}

async function ensureAuthRuntime({ processRedirectResult = false } = {}) {
  if (!authRuntimePromise) {
    authRuntimePromise = Promise.all([
      import(FIREBASE_APP_MODULE_URL),
      import(FIREBASE_AUTH_MODULE_URL),
      import(FIREBASE_CONFIG_MODULE_URL),
    ]).then(([firebaseApp, firebaseAuth, firebaseConfig]) => {
      const app = firebaseApp.initializeApp(firebaseConfig.getFirebaseConfig());
      authRuntime = {
        auth: firebaseAuth.getAuth(app),
        firebaseAuth,
      };
      bindAuthStateIfNeeded(authRuntime);
      return authRuntime;
    });
  }

  const runtime = await authRuntimePromise;
  if (processRedirectResult && !isAndroidWebView && !redirectResultHandled) {
    redirectResultHandled = true;
    runtime.firebaseAuth.getRedirectResult(runtime.auth).catch((err) => {
      console.error("Auth redirect error:", err?.code || "unknown");
      showToast("Authentication redirect failed. Please try again.");
    });
  }
  return runtime;
}

function handleAction(action) {
  try {
    action();
  } catch (error) {
    setStatus(error.message, STATUS_COLORS.error);
  }
}

function setButtonContent(button, iconClass, label) {
  button.innerHTML = `<i class="${iconClass}"></i> ${label}`;
}

function clearChatHistory() {
  pendingMessageNodes.length = 0;
  if (messageFlushHandle !== null) {
    cancelNextFrame(messageFlushHandle);
    messageFlushHandle = null;
  }
  messagesEl.replaceChildren();
}

function presenceEventKind(frame) {
  if (!frame || typeof frame.sender_id !== "string") {
    return null;
  }
  if (frame.kind === PRESENCE_JOIN_KIND || frame.kind === PRESENCE_LEAVE_KIND) {
    return frame.kind;
  }
  return null;
}

function sendPresenceJoin(senderId) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    return;
  }

  const event = {
    kind: PRESENCE_JOIN_KIND,
    sender_id: senderId,
    avatar_url: profileAvatarUrl(currentUser),
    created_at: Date.now(),
  };
  socket.send(JSON.stringify(event));
}

function appendSystemMessage(text) {
  const li = document.createElement("li");
  li.className = "message";

  const content = document.createElement("div");
  content.className = "message-content";

  const meta = document.createElement("p");
  meta.className = "meta";
  meta.textContent = "system";

  const body = document.createElement("p");
  body.className = "body";
  body.textContent = text;

  content.append(meta, body);
  li.append(content);
  queueMessageNode(li);
}

function appendMessage({ sender_id, payload_cipher, created_at, avatar_url }, isLocal = false) {
  const li = document.createElement("li");
  li.className = `message ${isLocal ? "me" : ""}`;

  const avatar = document.createElement("img");
  avatar.className = "message-avatar";
  avatar.src = messageAvatarUrl({ avatar_url, sender_id });
  avatar.alt = `${sender_id || "User"} avatar`;

  const content = document.createElement("div");
  content.className = "message-content";

  const meta = document.createElement("p");
  meta.className = "meta";
  const ts = new Date(created_at).toLocaleTimeString();
  meta.textContent = `${sender_id} • ${ts}`;

  const body = document.createElement("p");
  body.className = "body";
  body.textContent = decodeUtf8(payload_cipher) ?? toHex(payload_cipher);

  content.append(meta, body);
  li.append(avatar, content);
  queueMessageNode(li);
}

function scrollMessagesToBottom() {
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

function requestNextFrame(callback) {
  if (typeof globalThis.requestAnimationFrame === "function") {
    return globalThis.requestAnimationFrame(callback);
  }
  return globalThis.setTimeout(callback, 16);
}

function cancelNextFrame(handle) {
  if (typeof globalThis.cancelAnimationFrame === "function") {
    globalThis.cancelAnimationFrame(handle);
    return;
  }
  globalThis.clearTimeout(handle);
}

function queueMessageNode(node) {
  pendingMessageNodes.push(node);
  if (messageFlushHandle !== null) {
    return;
  }
  messageFlushHandle = requestNextFrame(flushPendingMessages);
}

function flushPendingMessages() {
  messageFlushHandle = null;
  if (pendingMessageNodes.length === 0) {
    return;
  }

  const fragment = document.createDocumentFragment();
  for (const node of pendingMessageNodes) {
    fragment.appendChild(node);
  }
  pendingMessageNodes.length = 0;
  messagesEl.appendChild(fragment);
  trimRenderedMessages();
  scrollMessagesToBottom();
}

function trimRenderedMessages() {
  while (messagesEl.childElementCount > MAX_RENDERED_MESSAGES) {
    messagesEl.firstElementChild?.remove();
  }
}

function toCipherBytes(plaintext) {
  return Array.from(textEncoder.encode(plaintext));
}

function decodeUtf8(bytes) {
  try {
    return textDecoder.decode(new Uint8Array(bytes));
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
    return JSON.parse(textDecoder.decode(new Uint8Array(data)));
  }
  if (data instanceof Blob) {
    const text = await data.text();
    return JSON.parse(text);
  }
  throw new Error("unsupported websocket frame type");
}

function buildRoomSocketUrl() {
  const base = baseUrlEl.value.trim().replace(/\/$/, "");
  const roomId = roomIdEl.value.trim();
  if (!base || !roomId) {
    throw new Error("Base URL and room ID are required");
  }

  if (!base.startsWith("ws://") && !base.startsWith("wss://")) {
    throw new Error("Base URL must start with ws:// or wss://");
  }

  return `${base}/room/${encodeURIComponent(roomId)}`;
}

function setConnectedState(connected) {
  connectBtn.disabled = false;
  disconnectBtn.disabled = !connected;
  messageInputEl.disabled = !connected;
  connectBtn.classList.toggle("is-connected", connected);
  setButtonContent(
    connectBtn,
    connected ? "fas fa-right-left" : "fas fa-plug",
    connected ? "Switch Room" : "Connect",
  );
  setButtonContent(disconnectBtn, "fas fa-link-slash", "Disconnect");
}

function initialsFromLabel(value) {
  const source = (value || "User").trim();
  const parts = source.split(/\s+/).filter(Boolean);
  if (parts.length >= 2) {
    return `${parts[0][0]}${parts[1][0]}`.toUpperCase();
  }
  return source.slice(0, 2).toUpperCase();
}

function avatarFallbackDataUrl(label) {
  const initials = initialsFromLabel(label);
  const svg = `
<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 96 96">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#89b4fa"/>
      <stop offset="100%" stop-color="#a6e3a1"/>
    </linearGradient>
  </defs>
  <rect width="96" height="96" rx="24" fill="url(#g)"/>
  <text x="50%" y="54%" dominant-baseline="middle" text-anchor="middle" font-size="34" font-family="sans-serif" font-weight="700" fill="#1e1e2e">${initials}</text>
</svg>`.trim();
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

function profileAvatarUrl(user) {
  if (user?.photoURL && user.photoURL.trim()) {
    return user.photoURL;
  }
  return avatarFallbackDataUrl(user?.displayName || user?.email || "User");
}

function messageAvatarUrl(message) {
  if (message?.avatar_url && message.avatar_url.trim()) {
    return message.avatar_url;
  }
  return avatarFallbackDataUrl(message?.sender_id || "User");
}

function setAuthUi(user) {
  if (user) {
    authPanelEl.classList.add("hidden");
    chatShellEl.classList.remove("hidden");
    const primary = user.displayName || user.email || user.uid;
    userChipEl.textContent = primary;
    userEmailEl.textContent = user.email || "No email available";
    userUidEl.textContent = `[${user.uid.slice(0, 8)}]`;
    profileAvatarEl.src = profileAvatarUrl(user);
    profileAvatarEl.alt = `${primary} avatar`;
    setStatus(`Ready. Sender ID: ${senderIdFor(user)}`);
    return;
  }

  authPanelEl.classList.remove("hidden");
  chatShellEl.classList.add("hidden");
  userChipEl.textContent = "Signed in";
  userEmailEl.textContent = "No email available";
  userUidEl.textContent = "[UID]";
  profileAvatarEl.src = avatarFallbackDataUrl("User");
  profileAvatarEl.alt = "Signed in user avatar";
}

function connect() {
  if (!currentUser) {
    throw new Error("Please sign in first");
  }
  if (socket && socket.readyState === WebSocket.OPEN) {
    setStatus(`Already connected to ${connectedRoomId || roomIdEl.value.trim()}`, STATUS_COLORS.info);
    return;
  }
  if (socket && socket.readyState === WebSocket.CONNECTING) {
    setStatus("Still connecting...", STATUS_COLORS.info);
    return;
  }

  const roomId = roomIdEl.value.trim();
  const wsUrl = buildRoomSocketUrl();
  const senderId = senderIdFor(currentUser);
  clearScheduledReconnect();
  connectBtn.disabled = true;
  disconnectBtn.disabled = false;
  messageInputEl.disabled = true;
  setButtonContent(connectBtn, "fas fa-spinner fa-spin", "Connecting...");
  socket = new WebSocket(wsUrl);
  socket.binaryType = "arraybuffer";

  setStatus(`Connecting to ${wsUrl}...`, STATUS_COLORS.info);

  socket.onopen = () => {
    reconnectAttemptCount = 0;
    setConnectedState(true);
    connectedRoomId = roomId;
    sendPresenceJoin(senderId);
    setStatus(`Connected to ${roomId} as ${senderId}`, STATUS_COLORS.success);
  };

  socket.onmessage = async (event) => {
    try {
      const incoming = await parseIncomingMessageData(event.data);
      const presenceKind = presenceEventKind(incoming);
      if (presenceKind === PRESENCE_JOIN_KIND) {
        if (incoming.sender_id !== senderId) {
          appendSystemMessage(`${incoming.sender_id} joined room ${connectedRoomId || roomId}`);
        }
        return;
      }
      if (presenceKind === PRESENCE_LEAVE_KIND) {
        if (incoming.sender_id !== senderId) {
          appendSystemMessage(`${incoming.sender_id} left room ${connectedRoomId || roomId}`);
        }
        return;
      }

      if (incoming.sender_id && Array.isArray(incoming.payload_cipher)) {
        appendMessage(incoming, incoming.sender_id === senderId);
      }
    } catch {
      setStatus("Received non-JSON message from server", STATUS_COLORS.error);
    }
  };

  socket.onerror = () => {
    setStatus(`WebSocket error at ${wsUrl}`, STATUS_COLORS.error);
  };

  socket.onclose = (event) => {
    const shouldReconnect = reconnectAfterClose;
    reconnectAfterClose = false;
    const closedByUserAction = manualCloseRequested;
    manualCloseRequested = false;
    connectedRoomId = "";
    setConnectedState(false);
    clearChatHistory();
    socket = null;

    if (shouldReconnect) {
      handleAction(connect);
      return;
    }

    const reason = event.reason ? ` (${event.reason})` : "";
    setStatus(`Disconnected [${event.code}]${reason}`, STATUS_COLORS.default);
    if (!closedByUserAction) {
      scheduleReconnect();
    } else {
      reconnectAttemptCount = 0;
      clearScheduledReconnect();
    }
  };
}

function disconnect(reconnect = false, closeReason = "Client closed connection") {
  manualCloseRequested = true;
  reconnectAfterClose = reconnect;
  clearScheduledReconnect();
  if (!socket) {
    setConnectedState(false);
    if (!reconnect) {
      clearChatHistory();
      setStatus("Already disconnected", STATUS_COLORS.info);
    }
    connectedRoomId = "";
    return;
  }

  if (
    socket.readyState === WebSocket.OPEN ||
    socket.readyState === WebSocket.CONNECTING
  ) {
    setStatus(
      reconnect ? "Switching rooms..." : "Disconnecting...",
      STATUS_COLORS.warning,
    );
    disconnectBtn.disabled = true;
    connectBtn.disabled = true;
    messageInputEl.disabled = true;
    setButtonContent(
      reconnect ? connectBtn : disconnectBtn,
      "fas fa-spinner fa-spin",
      reconnect ? "Switching..." : "Disconnecting...",
    );
  }

  if (socket.readyState !== WebSocket.CLOSED) {
    socket.close(1000, closeReason);
  }
}

function clearScheduledReconnect() {
  if (reconnectTimerId === null) {
    return;
  }
  globalThis.clearTimeout(reconnectTimerId);
  reconnectTimerId = null;
}

function nextReconnectDelayMs(attempt) {
  const exp = Math.min(RECONNECT_BASE_DELAY_MS * (2 ** attempt), RECONNECT_MAX_DELAY_MS);
  const jitter = exp * RECONNECT_JITTER_FACTOR * Math.random();
  return Math.round(exp + jitter);
}

function scheduleReconnect() {
  if (reconnectTimerId !== null || !currentUser) {
    return;
  }
  const room = roomIdEl.value.trim();
  if (!room) {
    return;
  }
  const delay = nextReconnectDelayMs(reconnectAttemptCount);
  reconnectAttemptCount += 1;
  const seconds = Math.max(1, Math.ceil(delay / 1000));
  setStatus(`Connection lost. Reconnecting in ${seconds}s...`, STATUS_COLORS.warning);

  reconnectTimerId = globalThis.setTimeout(() => {
    reconnectTimerId = null;
    handleAction(connect);
  }, delay);
}

function switchToSelectedRoom() {
  const selectedRoom = roomIdEl.value.trim();
  if (!selectedRoom) {
    throw new Error("Room ID is required");
  }
  buildRoomSocketUrl();

  if (!socket || socket.readyState === WebSocket.CLOSED) {
    clearChatHistory();
    connect();
    return;
  }

  if (selectedRoom === connectedRoomId && socket.readyState === WebSocket.OPEN) {
    setStatus(`Already in room ${selectedRoom}`, STATUS_COLORS.info);
    return;
  }

  clearChatHistory();
  disconnect(true, "Switching room");
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
    avatar_url: profileAvatarUrl(currentUser),
    payload_cipher: toCipherBytes(text),
    created_at: Date.now(),
  };

  socket.send(JSON.stringify(message));
  appendMessage(message, true);
}

const login = async () => {
  try {
    if (isAndroidWebView && hasAndroidNativeAuth) {
      globalThis.AndroidAuth.signInWithGoogle();
      return;
    }

    const runtime = await ensureAuthRuntime();
    const provider = new runtime.firebaseAuth.GoogleAuthProvider();
    provider.setCustomParameters({ prompt: "select_account" });

    if (isAndroidWebView) {
      await runtime.firebaseAuth.signInWithRedirect(runtime.auth, provider);
      return;
    }

    await runtime.firebaseAuth.signInWithPopup(runtime.auth, provider);
  } catch (err) {
    console.error("Auth error:", err?.code || "unknown");
    showToast("Authentication failed. Please try again.");
  }
};

googleBtn.onclick = () => {
  void login();
};

logoutBtn.onclick = async () => {
  try {
    disconnect();
    if (isAndroidWebView && hasAndroidNativeAuth) {
      globalThis.AndroidAuth.signOut();
      currentUser = null;
      setAuthUi(null);
      showToast("Signed out");
      return;
    }
    const runtime = await ensureAuthRuntime();
    await runtime.firebaseAuth.signOut(runtime.auth);
    showToast("Signed out");
  } catch (err) {
    console.error("Logout error:", err?.code || "unknown");
    showToast("Sign out failed. Please try again.");
  }
};

connectBtn.addEventListener("click", () => {
  handleAction(switchToSelectedRoom);
});

disconnectBtn.addEventListener("click", () => {
  disconnect();
});

roomIdEl.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") {
    return;
  }
  event.preventDefault();
  handleAction(switchToSelectedRoom);
});

roomIdEl.addEventListener("change", () => {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    return;
  }
  handleAction(switchToSelectedRoom);
});

composer.addEventListener("submit", (event) => {
  event.preventDefault();
  const text = messageInputEl.value.trim();
  if (!text) {
    return;
  }

  handleAction(() => {
    sendMessage(text);
    messageInputEl.value = "";
  });
});

setConnectedState(false);
setStatus("Please sign in to continue");

if (isAndroidWebView) {
  const existingAndroidUser = readAndroidCurrentUser();
  if (existingAndroidUser) {
    currentUser = existingAndroidUser;
    setAuthUi(existingAndroidUser);
    setStatus(`Ready. Sender ID: ${senderIdFor(existingAndroidUser)}`);
  }

  globalThis.onAndroidAuthResult = (raw) => {
    try {
      const parsed = typeof raw === "string" ? JSON.parse(raw) : raw;
      const user = normalizeAndroidUser(parsed);
      if (!user) {
        showToast("Android sign-in returned invalid user data.");
        return;
      }
      currentUser = user;
      setAuthUi(user);
      showToast(`Signed in as ${user.displayName || user.email || "user"}`);
    } catch {
      showToast("Failed to parse Android sign-in response.");
    }
  };

  globalThis.onAndroidSignedOut = () => {
    currentUser = null;
    disconnect();
    setAuthUi(null);
  };

  globalThis.onAndroidAuthError = (message) => {
    const text = typeof message === "string" && message.trim() ? message : "Android sign-in failed.";
    showToast(text);
    setStatus(text, STATUS_COLORS.error);
  };
}

if (!isAndroidWebView || !hasAndroidNativeAuth) {
  const initAuth = () => {
    void ensureAuthRuntime({ processRedirectResult: !isAndroidWebView });
  };
  if (typeof globalThis.requestIdleCallback === "function") {
    globalThis.requestIdleCallback(initAuth, { timeout: 1500 });
  } else {
    globalThis.setTimeout(initAuth, 0);
  }
}
