// App State
const appState = {
    wallet: null,
    walletAddress: null,
    connection: null,
    isRunning: false,
    stats: {
        detected: 0,
        bought: 0,
        filtered: 0,
    },
    feed: [],
    buys: [],
    settings: {
        privateKey: '',
        heliusApiKey: '',
        rpcUrl: 'https://mainnet.helius-rpc.com/?api-key=',
        wssUrl: '',
        buyAmount: 0.015,
        priorityFee: 11000000,
        computeUnits: 200000,
        jitoTip: 0.0015,
        solPrice: 162.0,
        targetMintAddress: null, // null = no target, string = target mint address
        minDevBuy: 500.0,
        maxDevBuy: 1200.0,
        minDevTokens: 6,
        maxDevTokens: 20,
        minSocialsCount: 0,
        requireSocials: false,
        requireTwitter: false,
        oneShotMode: true,
        enableTracker: false,
        submissionMode: 'helius',
    },
    ws: null,
};

// Initialize
document.addEventListener('DOMContentLoaded', () => {
    initializeApp();
    loadSettings();
    setupEventListeners();
});

function initializeApp() {
    // Initialize Solana connection
    if (typeof solanaWeb3 !== 'undefined') {
        appState.connection = new solanaWeb3.Connection(
            appState.settings.rpcUrl || 'https://api.mainnet-beta.solana.com',
            'confirmed'
        );
    }

    // Check for Phantom wallet
    if (window.solana && window.solana.isPhantom) {
        appState.wallet = window.solana;
    }
}

function setupEventListeners() {
    // Tab switching
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            const tab = btn.dataset.tab;
            switchTab(tab);
        });
    });

    // Wallet connection
    document.getElementById('connectWallet').addEventListener('click', connectWallet);

    // Start/Stop bot
    document.getElementById('startStopBtn').addEventListener('click', toggleBot);

    // Settings
    document.getElementById('saveSettings').addEventListener('click', saveSettings);
    document.getElementById('resetSettings').addEventListener('click', resetSettings);
    document.getElementById('togglePrivateKey').addEventListener('click', togglePrivateKey);
    document.getElementById('clearBuys').addEventListener('click', clearBuys);

    // Feed search and filter
    document.getElementById('feedSearch').addEventListener('input', filterFeed);
    document.getElementById('feedFilter').addEventListener('change', filterFeed);

    // Settings inputs - auto-save on change
    const settingsInputs = document.querySelectorAll('#settings input, #settings select');
    settingsInputs.forEach(input => {
        input.addEventListener('change', () => {
            updateSettingsFromUI();
        });
    });

    // Target mint - live validation and update
    const targetMintInput = document.getElementById('targetMint');
    targetMintInput.addEventListener('input', () => {
        validateAndUpdateTargetMint();
    });
}

function switchTab(tabName) {
    // Update tab buttons
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tabName);
    });

    // Update tab content
    document.querySelectorAll('.tab-content').forEach(content => {
        content.classList.toggle('active', content.id === tabName);
    });
}

async function connectWallet() {
    try {
        if (window.solana && window.solana.isPhantom) {
            const response = await window.solana.connect();
            appState.walletAddress = response.publicKey.toString();
            appState.wallet = window.solana;
            
            updateWalletUI();
            await updateWalletBalance();
            
            addActivity('Wallet connected: ' + shortenAddress(appState.walletAddress));
        } else if (appState.settings.privateKey) {
            // Use private key if provided
            appState.walletAddress = await getAddressFromPrivateKey(appState.settings.privateKey);
            updateWalletUI();
            addActivity('Wallet loaded from private key');
        } else {
            alert('Please install Phantom wallet or enter a private key in Settings');
        }
    } catch (err) {
        console.error('Wallet connection error:', err);
        addActivity('Wallet connection failed: ' + err.message, 'error');
    }
}

async function getAddressFromPrivateKey(privateKey) {
    // This would need to be implemented with a library like @solana/web3.js
    // For now, return a placeholder
    return 'Address from private key';
}

function updateWalletUI() {
    const connectBtn = document.getElementById('connectWallet');
    const walletInfo = document.getElementById('walletInfo');
    const walletAddress = document.getElementById('walletAddress');
    
    if (appState.walletAddress) {
        connectBtn.style.display = 'none';
        walletInfo.style.display = 'flex';
        walletAddress.textContent = shortenAddress(appState.walletAddress);
    } else {
        connectBtn.style.display = 'block';
        walletInfo.style.display = 'none';
    }
}

async function updateWalletBalance() {
    if (!appState.connection || !appState.walletAddress) return;
    
    try {
        const publicKey = new solanaWeb3.PublicKey(appState.walletAddress);
        const balance = await appState.connection.getBalance(publicKey);
        const solBalance = balance / 1e9;
        
        document.getElementById('walletBalance').textContent = solBalance.toFixed(4) + ' SOL';
    } catch (err) {
        console.error('Balance update error:', err);
    }
}

function toggleBot() {
    if (!appState.walletAddress) {
        alert('Please connect wallet first');
        return;
    }

    appState.isRunning = !appState.isRunning;
    
    const btn = document.getElementById('startStopBtn');
    const text = document.getElementById('startStopText');
    const statusBadge = document.getElementById('statStatus').querySelector('.status-badge');
    
    if (appState.isRunning) {
        btn.classList.add('stopped');
        text.textContent = 'Stop Bot';
        statusBadge.textContent = 'Running';
        statusBadge.className = 'status-badge running';
        startMonitoring();
        addActivity('Bot started');
    } else {
        btn.classList.remove('stopped');
        text.textContent = 'Start Bot';
        statusBadge.textContent = 'Stopped';
        statusBadge.className = 'status-badge stopped';
        stopMonitoring();
        addActivity('Bot stopped');
    }
}

function startMonitoring() {
    // Connect to WebSocket for token monitoring
    if (!appState.settings.wssUrl && !appState.settings.heliusApiKey) {
        addActivity('Error: Helius API key required for WebSocket connection', 'error');
        return;
    }

    const wssUrl = appState.settings.wssUrl || 
        `wss://mainnet.helius-rpc.com/?api-key=${appState.settings.heliusApiKey}`;
    
    try {
        appState.ws = new WebSocket(wssUrl);
        
        appState.ws.onopen = () => {
            addActivity('WebSocket connected');
            subscribeToTokenEvents();
        };
        
        appState.ws.onmessage = (event) => {
            handleTokenEvent(JSON.parse(event.data));
        };
        
        appState.ws.onerror = (error) => {
            addActivity('WebSocket error: ' + error.message, 'error');
        };
        
        appState.ws.onclose = () => {
            addActivity('WebSocket disconnected');
            if (appState.isRunning) {
                // Reconnect after 3 seconds
                setTimeout(startMonitoring, 3000);
            }
        };
    } catch (err) {
        addActivity('Failed to connect WebSocket: ' + err.message, 'error');
    }
}

function subscribeToTokenEvents() {
    // Subscribe to pump.fun program account changes
    // This is a simplified version - actual implementation would need proper subscription
    if (appState.ws && appState.ws.readyState === WebSocket.OPEN) {
        const subscription = {
            jsonrpc: '2.0',
            id: 1,
            method: 'accountSubscribe',
            params: [
                {
                    account: '6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P', // Pump.fun program ID
                    encoding: 'jsonParsed',
                },
                {
                    commitment: 'confirmed',
                },
            ],
        };
        appState.ws.send(JSON.stringify(subscription));
    }
}

function handleTokenEvent(data) {
    // Process token detection event
    // This is a simplified version - actual implementation would parse pump.fun events
    if (data.method === 'accountNotification') {
        const token = {
            mint: data.params.result.value.data.parsed?.info?.mint || 'Unknown',
            name: 'Token ' + Date.now(),
            timestamp: new Date(),
            status: 'detected',
        };
        
        processToken(token);
    }
}

function processToken(token) {
    // Check target mint first (if set)
    if (appState.settings.targetMintAddress) {
        if (token.mint !== appState.settings.targetMintAddress) {
            // Not the target token - filter it out
            token.status = 'filtered';
            addActivity(`Token filtered: Not target mint (waiting for: ${shortenAddress(appState.settings.targetMintAddress)})`, 'info');
            appState.stats.filtered++;
            updateStats();
            return; // Don't add to feed or process further
        } else {
            // Target token detected!
            addActivity(`🎯 Target token detected! ${shortenAddress(token.mint)}`, 'success');
        }
    }
    
    appState.stats.detected++;
    appState.feed.unshift(token);
    
    updateStats();
    updateFeed();
    addActivity(`Token detected: ${token.mint.substring(0, 8)}...`);
    
    // Apply filters
    if (shouldBuyToken(token)) {
        buyToken(token);
    } else {
        token.status = 'filtered';
        appState.stats.filtered++;
        updateStats();
        updateFeed();
    }
}

function shouldBuyToken(token) {
    // Simplified filter logic - actual implementation would check all criteria
    // This is a placeholder
    return Math.random() > 0.7; // 30% pass rate for demo
}

async function buyToken(token) {
    if (!appState.wallet || !appState.walletAddress) {
        addActivity('Cannot buy: Wallet not connected', 'error');
        return;
    }

    try {
        addActivity(`Attempting to buy token: ${token.mint.substring(0, 8)}...`);
        
        // This is a placeholder - actual implementation would:
        // 1. Build buy instruction
        // 2. Create transaction
        // 3. Sign and send
        
        // Simulate buy for demo
        setTimeout(() => {
            token.status = 'bought';
            appState.stats.bought++;
            appState.buys.unshift({
                ...token,
                amount: appState.settings.buyAmount,
                timestamp: new Date(),
            });
            
            updateStats();
            updateFeed();
            updateBuys();
            addActivity(`Successfully bought: ${token.mint.substring(0, 8)}...`, 'success');
        }, 1000);
        
    } catch (err) {
        addActivity('Buy failed: ' + err.message, 'error');
        token.status = 'filtered';
        appState.stats.filtered++;
        updateStats();
        updateFeed();
    }
}

function stopMonitoring() {
    if (appState.ws) {
        appState.ws.close();
        appState.ws = null;
    }
}

function updateStats() {
    document.getElementById('statDetected').textContent = appState.stats.detected;
    document.getElementById('statBought').textContent = appState.stats.bought;
    document.getElementById('statFiltered').textContent = appState.stats.filtered;
    
    const successRate = appState.stats.detected > 0 
        ? ((appState.stats.bought / appState.stats.detected) * 100).toFixed(1)
        : 0;
    document.getElementById('statSuccessRate').textContent = successRate + '%';
    
    const totalSpent = (appState.stats.bought * appState.settings.buyAmount).toFixed(4);
    document.getElementById('statSpent').textContent = totalSpent + ' SOL';
}

function updateFeed() {
    const feedList = document.getElementById('feedList');
    const searchTerm = document.getElementById('feedSearch').value.toLowerCase();
    const filter = document.getElementById('feedFilter').value;
    
    let filteredFeed = appState.feed.filter(item => {
        const matchesSearch = item.mint.toLowerCase().includes(searchTerm) ||
                             item.name.toLowerCase().includes(searchTerm);
        const matchesFilter = filter === 'all' || item.status === filter;
        return matchesSearch && matchesFilter;
    });
    
    if (filteredFeed.length === 0) {
        feedList.innerHTML = '<div class="feed-empty">No tokens match your filters.</div>';
        return;
    }
    
    feedList.innerHTML = filteredFeed.map(item => `
        <div class="feed-item">
            <div class="feed-item-header">
                <div class="token-info">
                    <div class="token-name">${item.name}</div>
                    <div class="token-mint">${shortenAddress(item.mint)}</div>
                </div>
            </div>
            <div class="token-status ${item.status}">${item.status.toUpperCase()}</div>
        </div>
    `).join('');
}

function updateBuys() {
    const buysList = document.getElementById('buysList');
    
    if (appState.buys.length === 0) {
        buysList.innerHTML = '<div class="buys-empty">No purchases yet.</div>';
        return;
    }
    
    buysList.innerHTML = appState.buys.map(buy => `
        <div class="buy-item">
            <div class="buy-info">
                <div class="buy-token">${buy.name}</div>
                <div class="buy-details">${shortenAddress(buy.mint)} • ${formatTime(buy.timestamp)}</div>
            </div>
            <div class="buy-amount">${buy.amount} SOL</div>
        </div>
    `).join('');
}

function addActivity(message, type = 'info') {
    const log = document.getElementById('activityLog');
    const time = new Date().toLocaleTimeString();
    
    const item = document.createElement('div');
    item.className = 'activity-item';
    item.innerHTML = `
        <span class="activity-time">${time}</span>
        <span class="activity-message" style="color: ${getActivityColor(type)}">${message}</span>
    `;
    
    log.insertBefore(item, log.firstChild);
    
    // Keep only last 50 items
    while (log.children.length > 50) {
        log.removeChild(log.lastChild);
    }
}

function getActivityColor(type) {
    switch (type) {
        case 'success': return 'var(--success)';
        case 'error': return 'var(--error)';
        case 'warning': return 'var(--warning)';
        default: return 'var(--text-secondary)';
    }
}

function filterFeed() {
    updateFeed();
}

function clearBuys() {
    if (confirm('Clear all purchase history?')) {
        appState.buys = [];
        updateBuys();
        addActivity('Purchase history cleared');
    }
}

function togglePrivateKey() {
    const input = document.getElementById('privateKey');
    const btn = document.getElementById('togglePrivateKey');
    
    if (input.type === 'password') {
        input.type = 'text';
        btn.textContent = 'Hide';
    } else {
        input.type = 'password';
        btn.textContent = 'Show';
    }
}

function validateAndUpdateTargetMint() {
    const input = document.getElementById('targetMint');
    const statusDiv = document.getElementById('targetMintStatus');
    const statusText = document.getElementById('targetMintStatusText');
    const value = input.value.trim();
    
    if (value === '') {
        // Empty - clear target
        appState.settings.targetMintAddress = null;
        statusDiv.style.display = 'none';
        input.style.borderColor = '';
        addActivity('Target mint cleared - bot will process all tokens');
        return;
    }
    
    // Validate Solana pubkey (basic check - 32-44 characters, base58)
    if (value.length < 32 || value.length > 44) {
        statusDiv.style.display = 'block';
        statusDiv.className = 'target-mint-status error';
        statusText.textContent = 'Invalid length (must be 32-44 characters)';
        input.style.borderColor = 'var(--error)';
        return;
    }
    
    // Try to validate as Solana pubkey using web3.js if available
    if (typeof solanaWeb3 !== 'undefined') {
        try {
            const pubkey = new solanaWeb3.PublicKey(value);
            // Valid pubkey
            appState.settings.targetMintAddress = value;
            statusDiv.style.display = 'block';
            statusDiv.className = 'target-mint-status success';
            statusText.textContent = `Target set: ${shortenAddress(value)}`;
            input.style.borderColor = 'var(--success)';
            addActivity(`🎯 Target mint set: ${shortenAddress(value)}`, 'success');
        } catch (err) {
            statusDiv.style.display = 'block';
            statusDiv.className = 'target-mint-status error';
            statusText.textContent = 'Invalid Solana address format';
            input.style.borderColor = 'var(--error)';
            return;
        }
    } else {
        // Fallback: basic validation
        const base58Regex = /^[1-9A-HJ-NP-Za-km-z]+$/;
        if (base58Regex.test(value)) {
            appState.settings.targetMintAddress = value;
            statusDiv.style.display = 'block';
            statusDiv.className = 'target-mint-status success';
            statusText.textContent = `Target set: ${shortenAddress(value)}`;
            input.style.borderColor = 'var(--success)';
            addActivity(`🎯 Target mint set: ${shortenAddress(value)}`, 'success');
        } else {
            statusDiv.style.display = 'block';
            statusDiv.className = 'target-mint-status error';
            statusText.textContent = 'Invalid format (must be base58)';
            input.style.borderColor = 'var(--error)';
            return;
        }
    }
}

function updateSettingsFromUI() {
    appState.settings.privateKey = document.getElementById('privateKey').value;
    appState.settings.heliusApiKey = document.getElementById('heliusApiKey').value;
    appState.settings.rpcUrl = document.getElementById('rpcUrl').value;
    appState.settings.wssUrl = document.getElementById('wssUrl').value;
    appState.settings.buyAmount = parseFloat(document.getElementById('buyAmount').value) || 0.015;
    appState.settings.priorityFee = parseInt(document.getElementById('priorityFee').value) || 11000000;
    appState.settings.computeUnits = parseInt(document.getElementById('computeUnits').value) || 200000;
    appState.settings.jitoTip = parseFloat(document.getElementById('jitoTip').value) || 0.0015;
    appState.settings.solPrice = parseFloat(document.getElementById('solPrice').value) || 162.0;
    // targetMintAddress is updated via validateAndUpdateTargetMint() on input
    appState.settings.minDevBuy = parseFloat(document.getElementById('minDevBuy').value) || 500.0;
    appState.settings.maxDevBuy = parseFloat(document.getElementById('maxDevBuy').value) || 1200.0;
    appState.settings.minDevTokens = parseInt(document.getElementById('minDevTokens').value) || 6;
    appState.settings.maxDevTokens = parseInt(document.getElementById('maxDevTokens').value) || 20;
    appState.settings.minSocialsCount = parseInt(document.getElementById('minSocialsCount').value) || 0;
    appState.settings.requireSocials = document.getElementById('requireSocials').checked;
    appState.settings.requireTwitter = document.getElementById('requireTwitter').checked;
    appState.settings.oneShotMode = document.getElementById('oneShotMode').checked;
    appState.settings.enableTracker = document.getElementById('enableTracker').checked;
    appState.settings.submissionMode = document.getElementById('submissionMode').value;
    
    // Update connection if RPC URL changed
    if (appState.settings.rpcUrl && typeof solanaWeb3 !== 'undefined') {
        appState.connection = new solanaWeb3.Connection(
            appState.settings.rpcUrl,
            'confirmed'
        );
    }
}

function saveSettings() {
    updateSettingsFromUI();
    localStorage.setItem('sniperSettings', JSON.stringify(appState.settings));
    addActivity('Settings saved');
    
    // Update wallet address if private key changed
    if (appState.settings.privateKey) {
        connectWallet();
    }
}

function loadSettings() {
    const saved = localStorage.getItem('sniperSettings');
    if (saved) {
        try {
            appState.settings = { ...appState.settings, ...JSON.parse(saved) };
        } catch (err) {
            console.error('Failed to load settings:', err);
        }
    }
    
    // Populate UI
    document.getElementById('privateKey').value = appState.settings.privateKey;
    document.getElementById('heliusApiKey').value = appState.settings.heliusApiKey;
    document.getElementById('rpcUrl').value = appState.settings.rpcUrl;
    document.getElementById('wssUrl').value = appState.settings.wssUrl;
    document.getElementById('buyAmount').value = appState.settings.buyAmount;
    document.getElementById('priorityFee').value = appState.settings.priorityFee;
    document.getElementById('computeUnits').value = appState.settings.computeUnits;
    document.getElementById('jitoTip').value = appState.settings.jitoTip;
    document.getElementById('solPrice').value = appState.settings.solPrice;
    document.getElementById('targetMint').value = appState.settings.targetMintAddress || '';
    if (appState.settings.targetMintAddress) {
        // Trigger validation to show status
        setTimeout(() => validateAndUpdateTargetMint(), 100);
    }
    document.getElementById('minDevBuy').value = appState.settings.minDevBuy;
    document.getElementById('maxDevBuy').value = appState.settings.maxDevBuy;
    document.getElementById('minDevTokens').value = appState.settings.minDevTokens;
    document.getElementById('maxDevTokens').value = appState.settings.maxDevTokens;
    document.getElementById('minSocialsCount').value = appState.settings.minSocialsCount;
    document.getElementById('requireSocials').checked = appState.settings.requireSocials;
    document.getElementById('requireTwitter').checked = appState.settings.requireTwitter;
    document.getElementById('oneShotMode').checked = appState.settings.oneShotMode;
    document.getElementById('enableTracker').checked = appState.settings.enableTracker;
    document.getElementById('submissionMode').value = appState.settings.submissionMode;
}

function resetSettings() {
    if (confirm('Reset all settings to defaults?')) {
        localStorage.removeItem('sniperSettings');
        appState.settings = {
            privateKey: '',
            heliusApiKey: '',
            rpcUrl: 'https://mainnet.helius-rpc.com/?api-key=',
            wssUrl: '',
            buyAmount: 0.015,
            priorityFee: 11000000,
            computeUnits: 200000,
            jitoTip: 0.0015,
            solPrice: 162.0,
            targetMintAddress: null,
            minDevBuy: 500.0,
            maxDevBuy: 1200.0,
            minDevTokens: 6,
            maxDevTokens: 20,
            minSocialsCount: 0,
            requireSocials: false,
            requireTwitter: false,
            oneShotMode: true,
            enableTracker: false,
            submissionMode: 'helius',
        };
        loadSettings();
        addActivity('Settings reset to defaults');
    }
}

// Utility functions
function shortenAddress(address) {
    if (!address) return '';
    return address.substring(0, 4) + '...' + address.substring(address.length - 4);
}

function formatTime(date) {
    return date.toLocaleTimeString();
}

// Periodic updates
setInterval(() => {
    if (appState.walletAddress) {
        updateWalletBalance();
    }
}, 10000); // Update every 10 seconds

