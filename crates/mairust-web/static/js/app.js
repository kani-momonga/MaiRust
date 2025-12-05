/**
 * MaiRust Web Client - Main Application JavaScript
 */

// Configuration
window.API_URL = '/api';

// Utility functions
const utils = {
    /**
     * Format a date for display
     */
    formatDate(dateStr) {
        if (!dateStr) return '';
        const date = new Date(dateStr);
        const now = new Date();
        const diff = now - date;

        // Today - show time
        if (diff < 86400000 && date.getDate() === now.getDate()) {
            return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
        }
        // Within a week - show day name
        if (diff < 86400000 * 7) {
            return date.toLocaleDateString([], { weekday: 'short' });
        }
        // Older - show date
        return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
    },

    /**
     * Format file size
     */
    formatSize(bytes) {
        if (!bytes) return '';
        if (bytes < 1024) return bytes + ' B';
        if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
        return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
    },

    /**
     * Get initials from email address
     */
    getInitials(email) {
        if (!email) return '?';
        const parts = email.split('@')[0].split('.');
        if (parts.length >= 2) {
            return (parts[0][0] + parts[1][0]).toUpperCase();
        }
        return email[0].toUpperCase();
    },

    /**
     * Debounce function
     */
    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    },

    /**
     * Show toast notification
     */
    showToast(message, type = 'success') {
        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;
        toast.textContent = message;
        document.body.appendChild(toast);

        setTimeout(() => {
            toast.style.opacity = '0';
            setTimeout(() => toast.remove(), 300);
        }, 3000);
    }
};

// API client
const api = {
    baseUrl: window.API_URL,

    async request(method, path, data = null) {
        const options = {
            method,
            headers: {
                'Content-Type': 'application/json',
            },
            credentials: 'include',
        };

        if (data) {
            options.body = JSON.stringify(data);
        }

        const response = await fetch(`${this.baseUrl}${path}`, options);

        if (!response.ok) {
            const error = await response.json().catch(() => ({ error: 'Request failed' }));
            throw new Error(error.error || 'Request failed');
        }

        return response.json();
    },

    get(path) {
        return this.request('GET', path);
    },

    post(path, data) {
        return this.request('POST', path, data);
    },

    put(path, data) {
        return this.request('PUT', path, data);
    },

    delete(path) {
        return this.request('DELETE', path);
    },

    // Messages
    async getMessages(params = {}) {
        const query = new URLSearchParams(params).toString();
        return this.get(`/messages${query ? '?' + query : ''}`);
    },

    async getMessage(id) {
        return this.get(`/messages/${id}`);
    },

    async updateMessage(id, data) {
        return this.put(`/messages/${id}`, data);
    },

    async deleteMessage(id) {
        return this.delete(`/messages/${id}`);
    },

    async sendMessage(data) {
        return this.post('/messages/send', data);
    },

    // Folders
    async getFolders() {
        return this.get('/folders');
    },

    async createFolder(name) {
        return this.post('/folders', { name });
    },

    async deleteFolder(name) {
        return this.delete(`/folders/${encodeURIComponent(name)}`);
    },

    // Tags
    async getTags() {
        return this.get('/tags');
    },

    async createTag(data) {
        return this.post('/tags', data);
    },

    async deleteTag(id) {
        return this.delete(`/tags/${id}`);
    },

    // Categories
    async getCategories() {
        return this.get('/categories');
    },

    // Settings
    async getSettings() {
        return this.get('/settings');
    },

    async updateSettings(data) {
        return this.put('/settings', data);
    },

    // Auth
    async login(email, password, remember = false) {
        return this.post('/auth/login', { email, password, remember });
    },

    async logout() {
        return this.post('/auth/logout');
    },

    async getProfile() {
        return this.get('/auth/profile');
    }
};

// WebSocket connection for real-time updates
class RealtimeConnection {
    constructor() {
        this.ws = null;
        this.listeners = new Map();
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
    }

    connect() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws`;

        try {
            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.reconnectAttempts = 0;
            };

            this.ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    this.emit(data.type, data.payload);
                } catch (e) {
                    console.error('Failed to parse WebSocket message:', e);
                }
            };

            this.ws.onclose = () => {
                console.log('WebSocket disconnected');
                this.scheduleReconnect();
            };

            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
            };
        } catch (e) {
            console.error('Failed to connect WebSocket:', e);
            this.scheduleReconnect();
        }
    }

    scheduleReconnect() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            const delay = Math.pow(2, this.reconnectAttempts) * 1000;
            this.reconnectAttempts++;
            console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
            setTimeout(() => this.connect(), delay);
        }
    }

    on(event, callback) {
        if (!this.listeners.has(event)) {
            this.listeners.set(event, []);
        }
        this.listeners.get(event).push(callback);
    }

    off(event, callback) {
        if (this.listeners.has(event)) {
            const callbacks = this.listeners.get(event);
            const index = callbacks.indexOf(callback);
            if (index > -1) {
                callbacks.splice(index, 1);
            }
        }
    }

    emit(event, data) {
        if (this.listeners.has(event)) {
            this.listeners.get(event).forEach(callback => callback(data));
        }
    }

    send(type, payload) {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({ type, payload }));
        }
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
}

// Global realtime connection instance
const realtime = new RealtimeConnection();

// Keyboard shortcuts
document.addEventListener('keydown', (e) => {
    // Only handle shortcuts when not in an input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
        return;
    }

    // Compose: c
    if (e.key === 'c' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        window.location.href = '/compose';
    }

    // Search: /
    if (e.key === '/' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        const searchInput = document.querySelector('input[type="search"]');
        if (searchInput) {
            searchInput.focus();
        }
    }

    // Go to inbox: g then i
    if (e.key === 'i' && window.lastKey === 'g') {
        e.preventDefault();
        window.location.href = '/inbox';
    }

    // Go to sent: g then s
    if (e.key === 's' && window.lastKey === 'g') {
        e.preventDefault();
        window.location.href = '/inbox?folder=sent';
    }

    window.lastKey = e.key;
    setTimeout(() => {
        if (window.lastKey === e.key) {
            window.lastKey = null;
        }
    }, 1000);
});

// Initialize on page load
document.addEventListener('DOMContentLoaded', () => {
    // Connect to realtime updates
    // realtime.connect(); // Uncomment when WebSocket endpoint is ready

    // Handle new message notifications
    realtime.on('new_message', (message) => {
        utils.showToast(`New message from ${message.from_address}`);
        // Refresh inbox if on inbox page
        if (window.location.pathname === '/inbox') {
            window.dispatchEvent(new CustomEvent('refresh-messages'));
        }
    });

    // Check authentication status
    // api.getProfile().catch(() => {
    //     if (window.location.pathname !== '/login') {
    //         window.location.href = '/login';
    //     }
    // });
});

// Export for use in templates
window.utils = utils;
window.api = api;
window.realtime = realtime;
