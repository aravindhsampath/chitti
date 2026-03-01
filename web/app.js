const wsUrl = `ws://${window.location.host}/ws`;
let ws;
let reconnectTimer;
let currentAssistantMessage = null;
let memoryEnabled = true;

const dom = {
    messages: document.getElementById('messages'),
    chatContainer: document.getElementById('chat-container'),
    input: document.getElementById('prompt-input'),
    sendBtn: document.getElementById('send-btn'),
    status: document.getElementById('connection-status'),
    toggleMemory: document.getElementById('toggle-memory'),
    clearContext: document.getElementById('clear-context')
};

marked.setOptions({
    highlight: function(code, lang) {
        if (lang && hljs.getLanguage(lang)) {
            return hljs.highlight(code, { language: lang }).value;
        }
        return hljs.highlightAuto(code).value;
    }
});

function connect() {
    ws = new WebSocket(wsUrl);

    ws.onopen = () => {
        dom.status.textContent = 'Connected';
        dom.status.className = 'connected';
        dom.sendBtn.disabled = dom.input.value.trim().length === 0;
        clearTimeout(reconnectTimer);
    };

    ws.onclose = () => {
        dom.status.textContent = 'Disconnected';
        dom.status.className = '';
        dom.sendBtn.disabled = true;
        reconnectTimer = setTimeout(connect, 2000);
    };

    ws.onerror = (err) => {
        console.error('WS Error', err);
        ws.close();
    };

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            handleSystemEvent(data);
        } catch (e) {
            console.error("Failed to parse message", e);
        }
    };
}

function handleSystemEvent(event) {
    let eventType = Object.keys(event)[0];
    if (typeof event === 'string') {
        eventType = event;
    }

    const payload = event[eventType];
    
    let state = null;
    if (Array.isArray(payload) && payload.length > 1) {
        state = payload[1];
    } else if (payload && payload.state) {
        state = payload.state;
    }

    if (state) {
        updateState(state);
    }

    switch (eventType) {
        case "Text": {
            const textDelta = payload[0];
            appendAssistantText(textDelta);
            break;
        }
        case "Thought": {
            const thoughtDelta = payload[0];
            appendAssistantThought(thoughtDelta);
            break;
        }
        case "Info": {
            const text = payload[0];
            if (text === "Context cleared.") {
                dom.messages.innerHTML = '';
                currentAssistantMessage = null;
            }
            appendSystemMessage(text);
            currentAssistantMessage = null;
            break;
        }
        case "Error": {
            const err = payload[0];
            appendErrorMessage(err);
            currentAssistantMessage = null;
            break;
        }
        case "ToolCall": {
            const { name, args } = payload;
            let argsStr = typeof args === 'object' ? JSON.stringify(args) : args;
            appendSystemMessage(`Tool Call: ${name}(${argsStr})`);
            break;
        }
        case "RequestApproval": {
            const { description } = payload;
            appendSystemMessage(`APPROVAL REQUIRED: ${description}`);
            appendSystemMessage("Type 'y' to approve, 'n' to reject, or steering instructions.");
            currentAssistantMessage = null;
            break;
        }
        case "Ready": {
            currentAssistantMessage = null;
            break;
        }
        case "Debug": {
            const text = payload[0];
            appendSystemMessage(`DEBUG: ${text}`);
            break;
        }
    }
    
    scrollToBottom();
}

function updateState(state) {
    memoryEnabled = state.memory_enabled;
    dom.toggleMemory.textContent = `🧠 Memory: ${memoryEnabled ? 'ON' : 'OFF'}`;
}

function createMessageElement(className) {
    const el = document.createElement('div');
    el.className = `message ${className}`;
    dom.messages.appendChild(el);
    return el;
}

function appendUserMessage(text) {
    const el = createMessageElement('user');
    el.textContent = text;
    currentAssistantMessage = null; 
}

function appendSystemMessage(text) {
    const el = createMessageElement('system');
    el.textContent = text;
}

function appendErrorMessage(text) {
    const el = createMessageElement('error');
    el.textContent = text;
}

function ensureAssistantMessage() {
    if (!currentAssistantMessage) {
        const el = createMessageElement('assistant');
        currentAssistantMessage = {
            el: el,
            rawText: '',
            rawThought: '',
            textEl: null,
            thoughtEl: null,
            thoughtContentEl: null
        };
    }
    return currentAssistantMessage;
}

function appendAssistantText(delta) {
    const msg = ensureAssistantMessage();
    msg.rawText += delta;
    
    if (!msg.textEl) {
        msg.textEl = document.createElement('div');
        msg.textEl.className = 'content-block';
        msg.el.appendChild(msg.textEl);
    }
    
    msg.textEl.innerHTML = DOMPurify.sanitize(marked.parse(msg.rawText));
    addCopyButtons(msg.textEl);
}

function appendAssistantThought(delta) {
    const msg = ensureAssistantMessage();
    msg.rawThought += delta;
    
    if (!msg.thoughtEl) {
        msg.thoughtEl = document.createElement('div');
        msg.thoughtEl.className = 'thought-block';
        
        const header = document.createElement('div');
        header.className = 'thought-header';
        header.textContent = 'Thinking Process';
        header.onclick = () => msg.thoughtEl.classList.toggle('open');
        
        msg.thoughtContentEl = document.createElement('div');
        msg.thoughtContentEl.className = 'thought-content';
        
        msg.thoughtEl.appendChild(header);
        msg.thoughtEl.appendChild(msg.thoughtContentEl);
        
        if (msg.textEl) {
            msg.el.insertBefore(msg.thoughtEl, msg.textEl);
        } else {
            msg.el.appendChild(msg.thoughtEl);
        }
    }
    
    msg.thoughtContentEl.textContent = msg.rawThought;
}

function addCopyButtons(container) {
    const blocks = container.querySelectorAll('pre');
    blocks.forEach(block => {
        if (block.querySelector('.copy-btn')) return;
        const btn = document.createElement('button');
        btn.className = 'copy-btn';
        btn.textContent = 'Copy';
        btn.onclick = () => {
            const code = block.querySelector('code');
            const text = code ? code.innerText : block.innerText;
            navigator.clipboard.writeText(text);
            btn.textContent = 'Copied!';
            setTimeout(() => btn.textContent = 'Copy', 2000);
        };
        block.appendChild(btn);
    });
}

function scrollToBottom() {
    dom.chatContainer.scrollTop = dom.chatContainer.scrollHeight;
}

function sendMessage() {
    const text = dom.input.value.trim();
    if (!text || ws.readyState !== WebSocket.OPEN) return;
    
    if (text === "/clear") {
         ws.send(text);
    } else if (text === "/memory") {
         ws.send(text);
    } else {
         appendUserMessage(text);
         ws.send(text);
    }
    
    dom.input.value = '';
    dom.input.style.height = 'auto';
    dom.sendBtn.disabled = true;
    scrollToBottom();
}

dom.sendBtn.onclick = sendMessage;

dom.input.oninput = () => {
    dom.sendBtn.disabled = dom.input.value.trim().length === 0 || ws.readyState !== WebSocket.OPEN;
    dom.input.style.height = 'auto';
    dom.input.style.height = (dom.input.scrollHeight) + 'px';
};

dom.input.onkeydown = (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        sendMessage();
    }
};

dom.toggleMemory.onclick = () => {
    if (ws.readyState === WebSocket.OPEN) {
        ws.send("/memory");
    }
};

dom.clearContext.onclick = () => {
    if (ws.readyState === WebSocket.OPEN) {
        ws.send("/clear");
    }
};

connect();