import { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { Activity, Shield, Zap, Box, Layers, Globe } from 'lucide-react';
import './App.css';

const DEFAULT_API_BASE = "https://haze-b3l9.onrender.com";

function App() {
  const [nodeStats, setNodeStats] = useState({
    height: "—",
    active_validators: "—",
    mempool_size: "—",
    tip_hash: "—",
    online: false
  });

  useEffect(() => {
    const fetchStats = async () => {
      try {
        const res = await fetch(DEFAULT_API_BASE + "/v1/status");
        if (res.ok) {
          const s = await res.json();
          setNodeStats({
            height: s.height.toString(),
            active_validators: (s.active_validators === 0 ? "1" : s.active_validators).toString(),
            mempool_size: s.mempool_size.toString(),
            tip_hash: (s.tip_hash || "").slice(0, 16) + "…",
            online: true
          });
        }
      } catch (e) {
        setNodeStats(prev => ({ ...prev, online: false }));
      }
    };

    fetchStats();
    const interval = setInterval(fetchStats, 8000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="app-container">
      {/* Navbar */}
      <header className="topbar">
        <a href="#top" className="brand">
          <span className="brand-mark">◖</span>
          <span className="brand-name">Haze</span>
        </a>
        <nav className="topnav">
          <a href="#protocol" className="navlink">Protocol</a>
          <a href="#mechanics" className="navlink">Mechanics</a>
          <a href="#tech" className="navlink">Under the hood</a>
          <a href="https://github.com/Pranav00x/haze" target="_blank" rel="noopener noreferrer" className="navlink">GitHub</a>
          <a href="https://wallet.hazeprotocol.xyz" target="_blank" rel="noopener noreferrer" className="btn-nav">Launch Wallet</a>
        </nav>
      </header>

      <main className="wrap" id="top">
        {/* Hero Section */}
        <section className="hero">
          <motion.div 
            className="hero-glow"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 1.5 }}
          />
          <div className="eyebrow">
            <span className="pulse-dot" /> Public testnet — live network
          </div>
          <motion.h1 
            className="hero-headline"
            initial={{ y: 20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            transition={{ duration: 0.6 }}
          >
            A ledger that doesn't <span className="grad-text">remember</span> who paid whom
          </motion.h1>
          <motion.p 
            className="hero-sub"
            initial={{ y: 20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            transition={{ duration: 0.6, delay: 0.1 }}
          >
            A ground-up Mimblewimble L1 built for absolute privacy. No accounts, no address books, and no smart contracts. Just pure math settling transactions via proof-of-stake.
          </motion.p>
          
          <motion.div 
            className="hero-ctas"
            initial={{ y: 20, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            transition={{ duration: 0.6, delay: 0.2 }}
          >
            <a href="https://wallet.hazeprotocol.xyz" target="_blank" rel="noopener noreferrer" className="btn btn-primary">Open the wallet →</a>
            <a href="#protocol" className="btn btn-ghost">Read the protocol spec</a>
          </motion.div>

          <motion.div 
            className="stats-strip"
            initial={{ y: 30, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            transition={{ duration: 0.8, delay: 0.3 }}
          >
            <div className="stat-cell node-indicator">
              <div className="st-label">
                <span className={`node-dot ${nodeStats.online ? 'online' : 'offline'}`}></span> 
                <span>Live Node</span>
              </div>
              <div className="st-value mono">{nodeStats.tip_hash}</div>
            </div>
            <div className="stat-cell">
              <div className="st-label">Block height</div>
              <div className="st-value">{nodeStats.height}</div>
            </div>
            <div className="stat-cell">
              <div className="st-label">Active validators</div>
              <div className="st-value">{nodeStats.active_validators}</div>
            </div>
            <div className="stat-cell">
              <div className="st-label">Mempool</div>
              <div className="st-value">{nodeStats.mempool_size}</div>
            </div>
          </motion.div>
        </section>

        {/* Protocol Section */}
        <section id="protocol">
          <div className="section-head">
            <div className="section-eyebrow">State model</div>
            <h2>No accounts. No balances. No transaction graph.</h2>
          </div>
          <div className="formula-panel">
            <p>Forget standard accounts. An <strong>output</strong> here is just a Pedersen commitment hiding the amount, a Bulletproof proving the math checks out, and a sealed note for the owner to decrypt later.</p>
            <p>Validating a block is one clean equation: inputs minus outputs plus fees equals zero. There are no balances to update and no addresses to route to. We just prove the spend is real without revealing the amounts or who's paying whom.</p>
            <p>Best part? Spent inputs and outputs <strong>cut through</strong>. They cancel each other out completely before they even land on the chain, keeping the history incredibly light.</p>
          </div>
        </section>

        {/* Mechanics Section */}
        <section id="mechanics">
          <div className="section-head">
            <div className="section-eyebrow">Shipped Mechanics</div>
            <h2>How it actually works</h2>
            <p>No roadmaps here. This is what's running on the live testnet today.</p>
          </div>
          <div className="mech-grid">
            <motion.div whileHover={{ y: -5 }} className="mech-card">
              <Globe className="mc-icon" />
              <div className="mc-num">01</div>
              <h3>Gossip that hides the source</h3>
              <p>Transactions route through Dandelion++. It's basically impossible to tell which node originally broadcast the payment.</p>
            </motion.div>
            <motion.div whileHover={{ y: -5 }} className="mech-card">
              <Activity className="mc-icon" />
              <div className="mc-num">02</div>
              <h3>Fair block proposals</h3>
              <p>If the main proposer misses their slot, any active validator can step up to keep the chain moving.</p>
            </motion.div>
            <motion.div whileHover={{ y: -5 }} className="mech-card">
              <Shield className="mc-icon" />
              <div className="mc-num">03</div>
              <h3>No guesswork on validators</h3>
              <p>Stake registration lives on-chain. Every node derives the exact same active validator set without needing to trust anyone.</p>
            </motion.div>
            <motion.div whileHover={{ y: -5 }} className="mech-card">
              <Layers className="mc-icon" />
              <div className="mc-num">04</div>
              <h3>Clean rollbacks</h3>
              <p>If a heavier fork comes along, Haze neatly unwinds and replays the state without breaking a sweat.</p>
            </motion.div>
            <motion.div whileHover={{ y: -5 }} className="mech-card">
              <Zap className="mc-icon" />
              <div className="mc-num">05</div>
              <h3>Predictable fees</h3>
              <p>You pay strictly based on the size of your transaction in bytes. No wild gas spikes just because the network is busy.</p>
            </motion.div>
            <motion.div whileHover={{ y: -5 }} className="mech-card">
              <Box className="mc-icon" />
              <div className="mc-num">06</div>
              <h3>Speak any protocol</h3>
              <p>TCP and WebSockets work out of the box. Nodes batch sync heavily so bringing a new peer online is fast.</p>
            </motion.div>
          </div>
        </section>

        {/* Tech Stack Section */}
        <section id="tech">
          <div className="section-head">
            <div className="section-eyebrow">Under the hood</div>
            <h2>One codebase to rule them all</h2>
            <p>The core logic has zero native dependencies. We compile the same Rust code straight down to WebAssembly, iOS, Android, and desktop.</p>
          </div>
          <div className="tech-grid">
            <div className="tech-cell">
              <div className="tc-label">The Crypto</div>
              <div className="tc-body">Ristretto255, Bulletproofs, and merlin transcripts. Everything is lean—absolutely no heavy pairing-based crypto in sight.</div>
            </div>
            <div className="tech-cell">
              <div className="tc-label">Tokenomics</div>
              <div className="tc-body">Hard capped at 21 billion. Most of it drops through block rewards that halve every four years.</div>
            </div>
            <div className="tech-cell">
              <div className="tc-label">Build Targets</div>
              <div className="tc-body">We test everywhere. The CI matrix builds for:
                <div className="target-pills">
                  <span className="target-pill">linux-x86_64</span>
                  <span className="target-pill">windows-msvc</span>
                  <span className="target-pill">macos-arm64</span>
                  <span className="target-pill">android</span>
                  <span className="target-pill">wasm32</span>
                </div>
              </div>
            </div>
          </div>
        </section>

      </main>

      {/* Footer */}
      <footer>
        <div className="wrap footer-row">
          <div className="fr-brand">
            <span className="brand-mark">◖</span>
            <span className="brand-name">Haze</span>
          </div>
          <div className="footer-links">
            <a href="Haze_Whitepaper.pdf" target="_blank" rel="noopener noreferrer">Whitepaper</a>
            <a href="https://github.com/Pranav00x/haze" target="_blank" rel="noopener noreferrer">GitHub</a>
            <a href="https://github.com/Pranav00x/haze/releases" target="_blank" rel="noopener noreferrer">Releases</a>
            <a href="mailto:pranav@hazeprotocol.xyz">Contact</a>
          </div>
        </div>
        <div className="wrap footer-note">
          This is public testnet software. HAZE has no monetary value, and the chain may still be reset if a serious bug is found — don't treat any balance here as real.
        </div>
      </footer>
    </div>
  );
}

export default App;
