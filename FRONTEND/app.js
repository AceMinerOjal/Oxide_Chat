const statusEl = document.getElementById('status');
const messagesEl = document.getElementById('messages');
const connectBtn = document.getElementById('connectBtn');
const disconnectBtn = document.getElementById('disconnectBtn');
const baseUrlEl = document.getElementById('baseUrl');
const roomIdEl = document.getElementById('roomId');
const composer = document.getElementById('composer');
const messageInputEl = document.getElementById('messageInput');

const senderId = `mobile-${crypto.randomUUID().slice(0, 8)}`;
let socket = null;

function setStatus(text, color = '#cdd6f4') {
  statusEl.textContent = text;
  statusEl.style.color = color;
}

function appendMessage({ sender_id, payload_cipher, created_at }, isLocal = false) {
  const li = document.createElement('li');
  li.className = `message ${isLocal ? 'me' : ''}`;

  const meta = document.createElement('p');
  meta.className = 'meta';
  const ts = new Date(created_at).toLocaleTimeString();
  meta.textContent = `${sender_id} • ${ts}`;

  const body = document.createElement('p');
  body.className = 'body';
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
  return bytes.map((b) => b.toString(16).padStart(2, '0')).join('');
}

function buildRoomSocketUrl() {
  const base = baseUrlEl.value.trim().replace(/\/$/, '');
  const roomId = roomIdEl.value.trim();
  if (!base || !roomId) {
    throw new Error('Base URL and room ID are required');
  }

  if (!base.startsWith('ws://') && !base.startsWith('wss://')) {
    throw new Error('Base URL must start with ws:// or wss://');
  }

  return `${base}/room/${encodeURIComponent(roomId)}`;
}

function setConnectedState(connected) {
  connectBtn.disabled = connected;
  disconnectBtn.disabled = !connected;
  messageInputEl.disabled = !connected;
}

function connect() {
  if (socket && socket.readyState === WebSocket.OPEN) {
    return;
  }

  const wsUrl = buildRoomSocketUrl();
  socket = new WebSocket(wsUrl);

  setStatus(`Connecting to ${wsUrl}...`, '#89b4fa');

  socket.onopen = () => {
    setConnectedState(true);
    setStatus(`Connected as ${senderId}`, '#a6e3a1');
  };

  socket.onmessage = (event) => {
    try {
      const text = typeof event.data === 'string' ? event.data : '';
      const incoming = JSON.parse(text);
      if (incoming.sender_id && Array.isArray(incoming.payload_cipher)) {
        appendMessage(incoming, incoming.sender_id === senderId);
      }
    } catch {
      setStatus('Received non-JSON message from server', '#f38ba8');
    }
  };

  socket.onerror = () => {
    setStatus('WebSocket error', '#f38ba8');
  };

  socket.onclose = () => {
    setConnectedState(false);
    setStatus('Disconnected', '#cdd6f4');
    socket = null;
  };
}

function disconnect() {
  if (socket) {
    socket.close(1000, 'Client closed connection');
  }
}

function sendMessage(text) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    throw new Error('No active WebSocket connection');
  }

  const message = {
    sender_id: senderId,
    payload_cipher: toCipherBytes(text),
    created_at: Date.now(),
  };

  socket.send(JSON.stringify(message));
  appendMessage(message, true);
}

connectBtn.addEventListener('click', () => {
  try {
    connect();
  } catch (error) {
    setStatus(error.message, '#f38ba8');
  }
});

disconnectBtn.addEventListener('click', () => {
  disconnect();
});

composer.addEventListener('submit', (event) => {
  event.preventDefault();
  const text = messageInputEl.value.trim();
  if (!text) {
    return;
  }

  try {
    sendMessage(text);
    messageInputEl.value = '';
  } catch (error) {
    setStatus(error.message, '#f38ba8');
  }
});

setConnectedState(false);
setStatus(`Ready. Sender ID: ${senderId}`);
