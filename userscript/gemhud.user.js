// ==UserScript==
// @name         GemHUD - BGA Splendor Card Values
// @namespace    https://github.com/Haro-stack/GemHUD
// @version      0.1.0
// @description  Read public BGA base Splendor card information and display local advisor value badges. Values only; no move automation.
// @author       Haro-stack
// @match        https://boardgamearena.com/*
// @match        https://*.boardgamearena.com/*
// @grant        GM_xmlhttpRequest
// @grant        GM_addStyle
// @grant        unsafeWindow
// @connect      127.0.0.1
// @connect      localhost
// @run-at       document-idle
// ==/UserScript==

(function gemhudUserscript() {
  "use strict";

  const APP = "GemHUD";
  const VERSION = "0.1.0";
  const DEFAULT_ENDPOINT = "http://127.0.0.1:8787/analyze";
  const STORAGE_ENDPOINT = "gemhud.endpoint";
  const STORAGE_ENABLED = "gemhud.enabled";
  const STORAGE_NOTICE = "gemhud.noticeAccepted";
  const BADGE_CLASS = "gemhud-value-badge";
  const CARD_MARK = "data-gemhud-client-id";

  const COLOR_ALIASES = [
    ["white", ["white", "w", "diamond", "blanc"]],
    ["blue", ["blue", "u", "sapphire", "bleu"]],
    ["green", ["green", "g", "emerald", "vert"]],
    ["red", ["red", "r", "ruby", "rouge"]],
    ["black", ["black", "b", "onyx", "noir"]],
  ];

  const CARD_SELECTOR = [
    "[data-card-id]",
    "[data-cardid]",
    "[id^='devcard']",
    "[id*='devcard']",
    "[id*='development']",
    "[class*='devcard']",
    "[class*='development'][class*='card']",
    "[class*='splendor'][class*='card']",
    "[class*='card'][style*='splendor']",
  ].join(",");

  const win = typeof unsafeWindow !== "undefined" ? unsafeWindow : window;
  let enabled = localStorage.getItem(STORAGE_ENABLED) !== "false";
  let lastRunAt = 0;
  let pendingTimer = 0;
  let scanSeq = 0;
  let lastCardElements = new Map();

  function addStyles() {
    GM_addStyle(`
      #gemhud-panel {
        position: fixed;
        right: 12px;
        bottom: 12px;
        z-index: 99999;
        width: 260px;
        color: #eaf1f7;
        background: rgba(19, 27, 33, 0.94);
        border: 1px solid rgba(128, 174, 196, 0.42);
        border-radius: 8px;
        box-shadow: 0 8px 28px rgba(0, 0, 0, 0.28);
        font-family: Arial, Helvetica, sans-serif;
        font-size: 12px;
        line-height: 1.35;
      }
      #gemhud-panel .gemhud-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        padding: 8px 10px;
        border-bottom: 1px solid rgba(128, 174, 196, 0.22);
        font-weight: 700;
      }
      #gemhud-panel .gemhud-body {
        padding: 8px 10px 10px;
      }
      #gemhud-panel .gemhud-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        margin-top: 8px;
      }
      #gemhud-panel button {
        border: 1px solid rgba(144, 189, 207, 0.42);
        border-radius: 6px;
        background: #263945;
        color: #f3f8fb;
        cursor: pointer;
        font: inherit;
        padding: 4px 8px;
      }
      #gemhud-panel button:hover {
        background: #304957;
      }
      #gemhud-panel input {
        box-sizing: border-box;
        width: 100%;
        border: 1px solid rgba(144, 189, 207, 0.35);
        border-radius: 6px;
        background: #101820;
        color: #f3f8fb;
        font: inherit;
        padding: 5px 6px;
      }
      #gemhud-panel .gemhud-status {
        color: #a9bdc9;
        min-height: 16px;
      }
      #gemhud-panel .gemhud-note {
        color: #9db1bc;
        margin-top: 6px;
      }
      .${BADGE_CLASS} {
        position: absolute;
        top: 4px;
        right: 4px;
        z-index: 99998;
        min-width: 42px;
        padding: 3px 5px;
        border-radius: 6px;
        background: rgba(13, 22, 28, 0.90);
        border: 1px solid rgba(255, 255, 255, 0.38);
        color: #ffffff;
        font-family: Arial, Helvetica, sans-serif;
        font-size: 11px;
        font-weight: 700;
        line-height: 1.1;
        text-align: center;
        pointer-events: none;
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
      }
      .${BADGE_CLASS}.gemhud-low { border-color: rgba(242, 120, 95, 0.75); }
      .${BADGE_CLASS}.gemhud-mid { border-color: rgba(245, 196, 92, 0.75); }
      .${BADGE_CLASS}.gemhud-high { border-color: rgba(94, 212, 146, 0.78); }
    `);
  }

  function endpoint() {
    return localStorage.getItem(STORAGE_ENDPOINT) || DEFAULT_ENDPOINT;
  }

  function isSplendorPage() {
    const href = location.href.toLowerCase();
    const body = (document.body && document.body.innerText || "").slice(0, 3000).toLowerCase();
    const gd = getGameDatas();
    const gameName = String((gd && (gd.game_name || gd.gamename || gd.game)) || "").toLowerCase();
    return href.includes("splendor") || gameName.includes("splendor") || body.includes("splendor");
  }

  function getGameDatas() {
    return win.g_gamedatas || win.gamedatas || win.gameui && win.gameui.gamedatas || null;
  }

  function sanitizeValue(value, depth) {
    if (depth > 5) return "[MaxDepth]";
    if (value == null) return value;
    if (["string", "number", "boolean"].includes(typeof value)) return value;
    if (Array.isArray(value)) return value.slice(0, 80).map((v) => sanitizeValue(v, depth + 1));
    if (typeof value !== "object") return String(value);

    const out = {};
    const entries = Object.entries(value).slice(0, 120);
    for (const [key, val] of entries) {
      const lk = key.toLowerCase();
      if (lk.includes("player_name") || lk === "name" || lk.includes("avatar") || lk.includes("token")) {
        continue;
      }
      out[key] = sanitizeValue(val, depth + 1);
    }
    return out;
  }

  function readText(el) {
    const parts = [];
    for (const attr of ["aria-label", "title", "alt", "data-tooltip", "data-card-name"]) {
      const v = el.getAttribute(attr);
      if (v) parts.push(v);
    }
    if (el.innerText) parts.push(el.innerText);
    return parts.join(" ").replace(/\s+/g, " ").trim();
  }

  function isVisible(el) {
    const rect = el.getBoundingClientRect();
    const style = window.getComputedStyle(el);
    return rect.width >= 24 && rect.height >= 24 && style.display !== "none" && style.visibility !== "hidden";
  }

  function isLikelySplendorCard(el) {
    if (!isVisible(el)) return false;
    if (el.closest("#gemhud-panel")) return false;
    const raw = [
      el.id,
      el.className && String(el.className),
      el.getAttribute("style"),
      readText(el),
      el.getAttribute("data-card-id"),
      el.getAttribute("data-cardid"),
    ].filter(Boolean).join(" ").toLowerCase();
    if (raw.includes("gemhud")) return false;
    if (raw.includes("splendor") || raw.includes("devcard") || raw.includes("development")) return true;
    if (el.hasAttribute("data-card-id") || el.hasAttribute("data-cardid")) return true;
    return false;
  }

  function parseNumber(pattern, source) {
    const m = source.match(pattern);
    if (!m) return null;
    const n = Number.parseInt(m[1], 10);
    return Number.isFinite(n) ? n : null;
  }

  function parseCost(source) {
    const lower = source.toLowerCase();
    const cost = {};
    for (const [color, aliases] of COLOR_ALIASES) {
      for (const alias of aliases) {
        const patterns = [
          new RegExp(`(?:${alias})\\s*[:=x-]?\\s*(\\d+)`, "i"),
          new RegExp(`(\\d+)\\s*(?:${alias})`, "i"),
          new RegExp(`cost[_-]?${alias}[^\\d]*(\\d+)`, "i"),
        ];
        const found = patterns.map((p) => parseNumber(p, lower)).find((n) => n !== null);
        if (found !== undefined) {
          cost[color] = found;
          break;
        }
      }
    }
    return cost;
  }

  function parseBonusColor(source) {
    const lower = source.toLowerCase();
    for (const [color, aliases] of COLOR_ALIASES) {
      if (aliases.some((alias) => lower.includes(`bonus_${alias}`) || lower.includes(`bonus-${alias}`))) {
        return color;
      }
    }
    for (const [color, aliases] of COLOR_ALIASES) {
      if (aliases.some((alias) => new RegExp(`\\b${alias}\\b`, "i").test(lower))) return color;
    }
    return null;
  }

  function parseTier(source) {
    return parseNumber(/(?:tier|level|row|deck)[_\s-]?([123])/i, source) ||
      parseNumber(/(?:card|devcard)[^\d]([123])[_-]/i, source);
  }

  function parsePoints(source) {
    return parseNumber(/(?:points?|prestige|vp)[^\d]*(\d+)/i, source) ||
      parseNumber(/\b(\d+)\s*(?:pts?|vp|prestige)\b/i, source);
  }

  function cardIdFromElement(el, fallbackIndex) {
    const attrs = [
      el.getAttribute("data-card-id"),
      el.getAttribute("data-cardid"),
      el.getAttribute("data-id"),
      el.id,
    ].filter(Boolean).join(" ");
    const m = attrs.match(/(?:card|devcard|id)[_-]?(\d+)/i) || attrs.match(/\b(\d{1,4})\b/);
    return m ? m[1] : `unknown-${fallbackIndex}`;
  }

  function extractDomCards() {
    const nodes = Array.from(document.querySelectorAll(CARD_SELECTOR));
    const dedup = new Set();
    const cards = [];
    lastCardElements = new Map();

    nodes.forEach((el, index) => {
      if (!(el instanceof HTMLElement) || !isLikelySplendorCard(el)) return;
      const source = [
        el.id,
        el.className && String(el.className),
        el.getAttribute("style"),
        readText(el),
        Array.from(el.attributes).map((a) => `${a.name}=${a.value}`).join(" "),
      ].filter(Boolean).join(" ");
      const rect = el.getBoundingClientRect();
      const id = cardIdFromElement(el, index);
      const clientId = el.getAttribute(CARD_MARK) || `dom:${id}:${Math.round(rect.left)}:${Math.round(rect.top)}`;
      if (dedup.has(clientId)) return;
      dedup.add(clientId);
      el.setAttribute(CARD_MARK, clientId);
      lastCardElements.set(clientId, el);
      cards.push({
        client_id: clientId,
        source: "dom",
        card_id: id,
        tier: parseTier(source),
        points: parsePoints(source),
        bonus_color: parseBonusColor(source),
        cost: parseCost(source),
        location: inferLocation(el),
        raw_text: readText(el).slice(0, 500),
        raw_hint: source.slice(0, 1000),
      });
    });
    return cards;
  }

  function inferLocation(el) {
    const ancestry = [];
    let cur = el;
    for (let i = 0; cur && i < 4; i += 1) {
      ancestry.push([cur.id, cur.className && String(cur.className)].filter(Boolean).join("."));
      cur = cur.parentElement;
    }
    const text = ancestry.join(" ").toLowerCase();
    if (text.includes("reserve")) return "reserved";
    if (text.includes("market") || text.includes("tableau") || text.includes("visible")) return "market";
    if (text.includes("noble")) return "noble";
    return "unknown";
  }

  function looksLikeCardObject(obj, path) {
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return false;
    const keys = Object.keys(obj).map((k) => k.toLowerCase());
    const joined = `${path.join(".").toLowerCase()} ${keys.join(" ")}`;
    const hasCardPath = /card|dev|tableau|market|visible|reserve/.test(joined);
    const hasCardFields = keys.some((k) => /cost|color|colour|bonus|point|prestige|level|type/.test(k));
    return hasCardPath && hasCardFields;
  }

  function extractDataCards() {
    const root = getGameDatas();
    const out = [];
    const seen = new Set();

    function walk(value, path, depth) {
      if (!value || depth > 6) return;
      if (Array.isArray(value)) {
        value.slice(0, 120).forEach((item, i) => walk(item, path.concat(String(i)), depth + 1));
        return;
      }
      if (typeof value !== "object") return;
      if (looksLikeCardObject(value, path)) {
        const cardId = String(value.id || value.card_id || value.cardId || value.type || path.join("_"));
        const clientId = `data:${path.join(".")}:${cardId}`;
        if (!seen.has(clientId)) {
          seen.add(clientId);
          out.push({
            client_id: clientId,
            source: "gamedatas",
            card_id: cardId,
            tier: Number(value.level || value.tier || value.row || value.deck) || null,
            points: Number(value.points || value.point || value.prestige || value.score) || null,
            bonus_color: String(value.color || value.colour || value.bonus || "").toLowerCase() || null,
            cost: normalizeCost(value.cost || value.costs || value),
            location: path.join("."),
            raw_text: "",
          });
        }
      }
      for (const [key, child] of Object.entries(value).slice(0, 120)) {
        walk(child, path.concat(key), depth + 1);
      }
    }

    walk(root, ["gamedatas"], 0);
    return out;
  }

  function normalizeCost(raw) {
    const cost = {};
    if (!raw || typeof raw !== "object") return cost;
    for (const [color, aliases] of COLOR_ALIASES) {
      for (const alias of aliases.concat(color)) {
        let value = raw[alias];
        if (value === undefined || value === null) value = raw[`cost_${alias}`];
        if (value === undefined || value === null) value = raw[`cost-${alias}`];
        const n = Number(value);
        if (Number.isFinite(n) && n >= 0) {
          cost[color] = n;
          break;
        }
      }
    }
    return cost;
  }

  function buildPayload() {
    const domCards = extractDomCards();
    const dataCards = extractDataCards();
    return {
      source: "bga",
      game: "splendor_base",
      version: VERSION,
      url: location.href,
      generated_at: new Date().toISOString(),
      capabilities: {
        values_only: true,
        automation: false,
        base_splendor_only: true,
      },
      cards: domCards.concat(dataCards),
      dom_card_count: domCards.length,
      public_context: sanitizeValue(getGameDatas(), 0),
    };
  }

  function postJson(url, payload) {
    return new Promise((resolve, reject) => {
      const body = JSON.stringify(payload);
      if (typeof GM_xmlhttpRequest === "function") {
        GM_xmlhttpRequest({
          method: "POST",
          url,
          data: body,
          headers: {"Content-Type": "application/json"},
          timeout: 10000,
          onload: (res) => {
            try {
              resolve(JSON.parse(res.responseText || "{}"));
            } catch (err) {
              reject(new Error(`Advisor returned invalid JSON: ${err.message}`));
            }
          },
          onerror: () => reject(new Error("Advisor request failed")),
          ontimeout: () => reject(new Error("Advisor request timed out")),
        });
        return;
      }
      fetch(url, {
        method: "POST",
        headers: {"Content-Type": "application/json"},
        body,
      }).then((r) => r.json()).then(resolve, reject);
    });
  }

  function setStatus(text) {
    const el = document.querySelector("#gemhud-status");
    if (el) el.textContent = text;
  }

  function renderBadges(response) {
    document.querySelectorAll(`.${BADGE_CLASS}`).forEach((el) => el.remove());
    const values = Array.isArray(response && response.cards) ? response.cards : [];
    let rendered = 0;
    for (const item of values) {
      const el = lastCardElements.get(item.client_id);
      if (!el) continue;
      const score = Number(item.value);
      if (!Number.isFinite(score)) continue;
      if (window.getComputedStyle(el).position === "static") {
        el.style.position = "relative";
      }
      const badge = document.createElement("div");
      badge.className = `${BADGE_CLASS} ${score >= 0.66 ? "gemhud-high" : score >= 0.33 ? "gemhud-mid" : "gemhud-low"}`;
      const pct = Math.round(score * 100);
      badge.textContent = `${pct}`;
      badge.title = `GemHUD value ${pct}/100. Method: ${item.method || "local advisor"}. Values only; no automation.`;
      el.appendChild(badge);
      rendered += 1;
    }
    return rendered;
  }

  async function runScan(reason) {
    if (!enabled) {
      setStatus("Disabled");
      return;
    }
    if (!isSplendorPage()) {
      setStatus("Waiting for BGA Splendor");
      return;
    }
    const seq = ++scanSeq;
    const now = Date.now();
    if (now - lastRunAt < 2000 && reason !== "manual") return;
    lastRunAt = now;

    const payload = buildPayload();
    if (payload.dom_card_count === 0) {
      setStatus("No visible cards detected");
      return;
    }

    setStatus(`Sending ${payload.dom_card_count} cards`);
    try {
      const response = await postJson(endpoint(), payload);
      if (seq !== scanSeq) return;
      const count = renderBadges(response);
      const mode = response && response.engine ? response.engine : "advisor";
      setStatus(`Rendered ${count} values (${mode})`);
    } catch (err) {
      setStatus(`Advisor offline: ${err.message}`);
    }
  }

  function scheduleScan(reason) {
    window.clearTimeout(pendingTimer);
    pendingTimer = window.setTimeout(() => runScan(reason), 600);
  }

  function buildPanel() {
    const panel = document.createElement("div");
    panel.id = "gemhud-panel";
    panel.innerHTML = `
      <div class="gemhud-head">
        <span>GemHUD</span>
        <button type="button" id="gemhud-toggle">${enabled ? "On" : "Off"}</button>
      </div>
      <div class="gemhud-body">
        <input id="gemhud-endpoint" value="${escapeHtml(endpoint())}" aria-label="GemHUD advisor endpoint" />
        <div class="gemhud-row">
          <span id="gemhud-status" class="gemhud-status">Idle</span>
          <button type="button" id="gemhud-scan">Scan</button>
        </div>
        <div class="gemhud-note">Base Splendor public card values only. No automatic moves.</div>
      </div>
    `;
    document.body.appendChild(panel);
    panel.querySelector("#gemhud-toggle").addEventListener("click", () => {
      enabled = !enabled;
      localStorage.setItem(STORAGE_ENABLED, enabled ? "true" : "false");
      panel.querySelector("#gemhud-toggle").textContent = enabled ? "On" : "Off";
      if (!enabled) document.querySelectorAll(`.${BADGE_CLASS}`).forEach((el) => el.remove());
      scheduleScan("manual");
    });
    panel.querySelector("#gemhud-scan").addEventListener("click", () => runScan("manual"));
    panel.querySelector("#gemhud-endpoint").addEventListener("change", (event) => {
      localStorage.setItem(STORAGE_ENDPOINT, event.target.value.trim() || DEFAULT_ENDPOINT);
      runScan("manual");
    });
  }

  function escapeHtml(text) {
    return String(text).replace(/[&<>"']/g, (ch) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "\"": "&quot;",
      "'": "&#39;",
    }[ch]));
  }

  function showNoticeOnce() {
    if (localStorage.getItem(STORAGE_NOTICE) === "true") return;
    localStorage.setItem(STORAGE_NOTICE, "true");
    window.alert("GemHUD only displays local value estimates for practice. It does not click, submit, or automate BGA actions.");
  }

  function start() {
    if (!document.body) return;
    addStyles();
    buildPanel();
    showNoticeOnce();
    scheduleScan("initial");
    const observer = new MutationObserver(() => scheduleScan("mutation"));
    observer.observe(document.body, {childList: true, subtree: true, attributes: true});
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start, {once: true});
  } else {
    start();
  }
})();
