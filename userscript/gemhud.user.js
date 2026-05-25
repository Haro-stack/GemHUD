// ==UserScript==
// @name         GemHUD - BGA Splendor Card Values
// @namespace    https://github.com/Haro-stack/GemHUD
// @version      0.1.0
// @description  Read public BGA base Splendor card information and display local advisor value badges. Values only; no move automation.
// @author       Haro-stack
// @match        https://boardgamearena.com/*
// @match        https://*.boardgamearena.com/*
// @match        https://game.hullqin.cn/ccbs/*
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
  const META_CLASS = "gemhud-card-meta";
  const CARD_MARK = "data-gemhud-client-id";

  const COLOR_ALIASES = [
    ["white", ["white", "w", "diamond", "blanc"]],
    ["blue", ["blue", "u", "sapphire", "bleu"]],
    ["green", ["green", "g", "emerald", "vert"]],
    ["red", ["red", "r", "ruby", "rouge"]],
    ["black", ["black", "b", "onyx", "noir"]],
  ];

  const BGA_COST_COLORS = {
    C: "white",
    S: "blue",
    E: "green",
    R: "red",
    O: "black",
    G: "gold",
  };

  const BGA_TYPE_COLORS = {
    0: "white",
    1: "blue",
    2: "green",
    3: "red",
    4: "black",
    5: "wild",
    6: "gold",
  };

  const COLOR_LABELS = {
    white: "W",
    blue: "U",
    green: "G",
    red: "R",
    black: "B",
    gold: "Au",
    wild: "*",
  };

  const COST_ORDER = ["white", "blue", "green", "red", "black", "gold"];
  const HULL_COLOR_LABELS = ["W", "U", "G", "R", "B", "Au"];
  const HULL_BONUS_COLORS = ["white", "blue", "green", "red", "black"];
  const HULL_NOBLES = [
    [0, 0, 4, 4, 0],
    [0, 4, 4, 0, 0],
    [4, 4, 0, 0, 0],
    [4, 0, 0, 0, 4],
    [0, 0, 0, 4, 4],
    [3, 3, 0, 0, 3],
    [0, 0, 3, 3, 3],
    [3, 0, 0, 3, 3],
    [0, 3, 3, 3, 0],
    [3, 3, 3, 0, 0],
  ];
  const HULL_CARD_POINTS = (() => {
    const base = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 4, 5];
    return base.concat(base, base, base, base);
  })();
  const HULL_CARD_BONUS = [
    ...Array(18).fill(0),
    ...Array(18).fill(1),
    ...Array(18).fill(2),
    ...Array(18).fill(3),
    ...Array(18).fill(4),
  ];
  const HULL_CARD_TIER = (() => {
    const base = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2];
    return base.concat(base, base, base, base);
  })();
  const HULL_CARD_IMAGE = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 4, 3, 3, 4,
    0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 1, 1, 2, 1, 4, 4, 3, 3,
    0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 1, 1, 1, 3, 4, 4, 3,
    0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 1, 1, 1, 4, 4, 3, 3,
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 4, 4, 3,
  ];
  const HULL_CARD_COSTS = (() => {
    const base = [
      [0, 1, 1, 1, 1], [0, 1, 2, 1, 1], [3, 1, 0, 0, 1], [0, 2, 2, 0, 1],
      [0, 2, 0, 0, 2], [0, 0, 0, 2, 1], [0, 3, 0, 0, 0], [0, 0, 4, 0, 0],
      [2, 3, 0, 3, 0], [0, 0, 3, 2, 2], [0, 0, 1, 4, 2], [0, 0, 0, 5, 0],
      [0, 0, 0, 5, 3], [6, 0, 0, 0, 0], [0, 3, 3, 5, 3], [0, 0, 0, 0, 7],
      [3, 0, 0, 3, 6], [3, 0, 0, 0, 7],
    ];
    const out = base.map((row) => row.slice());
    let working = base.map((row) => row.slice());
    for (let turn = 1; turn < 5; turn += 1) {
      working = working.map((row) => [row[4], row[0], row[1], row[2], row[3]]);
      out.push(...working.map((row) => row.slice()));
    }
    Object.assign(out, {
      20: [0, 1, 3, 1, 0], 22: [0, 0, 2, 0, 2], 24: [0, 0, 0, 0, 3],
      27: [0, 2, 2, 3, 0], 29: [0, 5, 0, 0, 0], 30: [5, 3, 0, 0, 0],
      38: [1, 3, 1, 0, 0], 45: [2, 3, 0, 0, 2], 47: [0, 0, 5, 0, 0],
      48: [0, 5, 3, 0, 0], 56: [1, 0, 0, 1, 3], 58: [2, 0, 0, 2, 0],
      60: [3, 0, 0, 0, 0], 63: [2, 0, 0, 2, 3], 65: [0, 0, 0, 0, 5],
      66: [3, 0, 0, 0, 5], 74: [0, 0, 1, 3, 1], 76: [2, 0, 2, 0, 0],
      78: [0, 0, 3, 0, 0], 81: [3, 2, 2, 0, 0], 83: [5, 0, 0, 0, 0],
    });
    return out;
  })();

  const CARD_SELECTOR = [
    "[data-card-id]",
    "[data-cardid]",
    ".spl_card",
    "[id^='card_']",
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
  let lastCardMeta = new Map();

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
      #gemhud-panel .gemhud-reco {
        margin-top: 7px;
        padding: 6px 7px;
        border-radius: 6px;
        background: rgba(92, 164, 201, 0.12);
        border: 1px solid rgba(128, 197, 231, 0.24);
        color: #dff5ff;
        font-weight: 700;
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
      .${META_CLASS} {
        position: absolute;
        left: 4px;
        bottom: 4px;
        z-index: 99997;
        box-sizing: border-box;
        max-width: calc(100% - 8px);
        padding: 2px 5px;
        border-radius: 5px;
        background: rgba(13, 22, 28, 0.84);
        border: 1px solid rgba(255, 255, 255, 0.30);
        color: #f4f8fb;
        font-family: Arial, Helvetica, sans-serif;
        font-size: 10px;
        font-weight: 700;
        line-height: 1.15;
        pointer-events: none;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.48);
      }
    `);
  }

  function endpoint() {
    return localStorage.getItem(STORAGE_ENDPOINT) || DEFAULT_ENDPOINT;
  }

  function isSplendorPage() {
    if (isHullQinCcbsPage()) return true;
    const href = location.href.toLowerCase();
    const body = (document.body && document.body.innerText || "").slice(0, 3000).toLowerCase();
    const gd = getGameDatas();
    const gameName = String((gd && (gd.game_name || gd.gamename || gd.game)) || "").toLowerCase();
    return href.includes("splendor") || gameName.includes("splendor") || body.includes("splendor");
  }

  function isHullQinCcbsPage() {
    return location.hostname === "game.hullqin.cn" && /^\/ccbs\//i.test(location.pathname);
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

  function currentBgaCardRoot(el) {
    if (!(el instanceof HTMLElement)) return null;
    const root = el.matches(".spl_card[id^='card_']")
      ? el
      : el.closest(".spl_card[id^='card_']");
    if (!(root instanceof HTMLElement)) return null;
    if (!/^card_\d+$/.test(root.id || "")) return null;
    return root;
  }

  function isLikelySplendorCard(el) {
    if (!isVisible(el)) return false;
    if (el.closest("#gemhud-panel")) return false;
    const bgaRoot = currentBgaCardRoot(el);
    if (bgaRoot) return bgaRoot === el && isVisible(bgaRoot);
    if (el.classList && el.classList.contains("spl_card")) return false;
    const raw = [
      el.id,
      el.className && String(el.className),
      el.getAttribute("style"),
      readText(el),
      el.getAttribute("data-card-id"),
      el.getAttribute("data-cardid"),
    ].filter(Boolean).join(" ").toLowerCase();
    if (raw.includes("gemhud")) return false;
    if (raw.includes("spl_cardcost") || raw.includes("spl_cardheader") || raw.includes("card_tucker")) return false;
    if (raw.includes("splendor") || raw.includes("devcard") || raw.includes("development")) return true;
    if (/\bspl_card\b/.test(raw) || /^card_\d+$/.test(el.id || "")) return true;
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

  function bgaTierFromElement(el) {
    const row = el.closest("[id^='row_']");
    const rowTier = row && String(row.id || "").match(/^row_([123])$/);
    if (rowTier) return Number(rowTier[1]);
    const drawpile = el.closest("[id^='drawpile']");
    const drawpileTier = drawpile && String(drawpile.id || "").match(/^drawpile([123])$/);
    return drawpileTier ? Number(drawpileTier[1]) : null;
  }

  function parsePoints(source) {
    return parseNumber(/(?:points?|prestige|vp)[^\d]*(\d+)/i, source) ||
      parseNumber(/\b(\d+)\s*(?:pts?|vp|prestige)\b/i, source);
  }

  function bgaCardDb() {
    const gd = getGameDatas();
    const gameui = win.gameui || {};
    const candidates = [
      gd && gd.carddb,
      gd && gd.cardDb,
      gd && gd.card_db,
      gd && gd.cardsdb,
      gd && gd.cards_db,
      gd && gd.cardtypes,
      gd && gd.card_types,
      gameui.carddb,
      gameui.cardDb,
      gameui.card_types,
      win.carddb,
    ];
    return candidates.find((value) => value && typeof value === "object" && !Array.isArray(value)) || null;
  }

  function bgaCardRecord(cardId) {
    const db = bgaCardDb();
    if (!db) return null;
    const id = String(cardId || "").replace(/^(?:card|devcard)[_-]?/i, "");
    return db[id] || db[Number(id)] || null;
  }

  function normalizeBgaLevel(raw) {
    const level = Number(raw);
    if (!Number.isFinite(level)) return null;
    if (level >= 11 && level <= 13) return level - 10;
    if (level >= 1 && level <= 3) return level;
    return null;
  }

  function countBgaCostLetters(raw) {
    const cost = {};
    for (const ch of String(raw || "").toUpperCase()) {
      const color = BGA_COST_COLORS[ch];
      if (!color) continue;
      cost[color] = (cost[color] || 0) + 1;
    }
    return cost;
  }

  function normalizeBgaCardRecord(record) {
    if (!record || typeof record !== "object") return null;
    const points = Number(record.points);
    const bonusColor = BGA_TYPE_COLORS[String(record.type)] || null;
    const cost = countBgaCostLetters(record.cost);
    const cardCost = countBgaCostLetters(record.costCard);
    const normalized = {
      tier: normalizeBgaLevel(record.lvl || record.level || record.tier),
      points: Number.isFinite(points) ? points : 0,
      bonus_color: bonusColor,
      cost,
      bga_carddb: sanitizeValue(record, 0),
      bga_carddb_found: true,
      ability_labels: bgaAbilityLabels(record, bonusColor, cardCost),
      card_cost: cardCost,
    };
    return normalized;
  }

  function bgaAbilityLabels(record, bonusColor, cardCost) {
    const labels = [];
    const level = Number(record.lvl || record.level || record.tier);
    const nbBonus = Number(record.nbBonus || 0);
    const symbolTake = Number(record.symbolTake || 0);
    if (bonusColor === "gold") labels.push("Au2");
    if (Number(record.symbolCopy || 0) > 0) labels.push("Copy");
    if (symbolTake > 0) labels.push(`Free T${symbolTake}`);
    if (nbBonus > 1 && bonusColor && bonusColor !== "wild" && bonusColor !== "gold") labels.push(`${nbBonus}x`);
    if (Object.keys(cardCost).length > 0) labels.push(`Discard ${formatCompactCost(cardCost)}`);
    if (level >= 11 && level <= 13 && labels.length === 0) labels.push("Orient");
    return labels;
  }

  function nonZeroCostEntries(cost) {
    return COST_ORDER
      .map((color) => [color, Number(cost && cost[color]) || 0])
      .filter(([, count]) => count > 0);
  }

  function formatCompactCost(cost) {
    const entries = nonZeroCostEntries(cost);
    if (!entries.length) return "Free";
    return entries.map(([color, count]) => `${COLOR_LABELS[color] || color[0].toUpperCase()}${count}`).join(" ");
  }

  function compactCardSummary(card) {
    const parts = [];
    const points = Number(card && card.points);
    parts.push(`${Number.isFinite(points) ? points : 0}P`);
    if (card && card.bonus_color) parts.push(COLOR_LABELS[card.bonus_color] || card.bonus_color);
    if (card && card.cost && nonZeroCostEntries(card.cost).length > 0) {
      parts.push(formatCompactCost(card.cost));
    }
    if (card && Array.isArray(card.ability_labels) && card.ability_labels.length) {
      parts.push(card.ability_labels.join("/"));
    }
    return parts.join(" ");
  }

  function verboseCardSummary(card) {
    if (!card) return "";
    const lines = [];
    lines.push(`Card ${card.card_id || "unknown"}`);
    if (card.tier) lines.push(`Tier ${card.tier}`);
    lines.push(`Prestige ${Number.isFinite(Number(card.points)) ? Number(card.points) : 0}`);
    if (card.bonus_color) lines.push(`Bonus ${card.bonus_color}`);
    lines.push(`Cost ${formatCompactCost(card.cost || {})}`);
    if (Array.isArray(card.ability_labels) && card.ability_labels.length) {
      lines.push(`Ability ${card.ability_labels.join(", ")}`);
    }
    return lines.join("\n");
  }

  function toInt(value, fallback = 0) {
    const n = Number.parseInt(String(value == null ? "" : value), 10);
    return Number.isFinite(n) ? n : fallback;
  }

  function bgaCardEntries(container) {
    if (!container) return [];
    if (Array.isArray(container)) return container.filter(Boolean);
    if (typeof container === "object") return Object.values(container).filter(Boolean);
    return [];
  }

  function sortedBgaMarketCards(tier) {
    const gd = getGameDatas();
    const row = gd && gd.market && gd.market[`row_${tier}`];
    return bgaCardEntries(row && row.cards).sort((a, b) => {
      const aSlot = toInt(a.location_arg, 999);
      const bSlot = toInt(b.location_arg, 999);
      if (aSlot !== bSlot) return aSlot - bSlot;
      return toInt(a.id || a.type, 0) - toInt(b.id || b.type, 0);
    });
  }

  function bgaMarketPosition(cardId) {
    const id = String(cardId || "");
    for (let tier = 1; tier <= 3; tier += 1) {
      const rowCards = sortedBgaMarketCards(tier);
      const slot = rowCards.findIndex((card) => String(card.id || card.type) === id || String(card.type) === id);
      if (slot >= 0) {
        return {
          tier,
          tier_index: tier - 1,
          slot_index: slot,
          market_index: (tier - 1) * 4 + slot,
        };
      }
    }
    return null;
  }

  function vectorFromBgaColorCounts(raw, includeGold = true) {
    const out = [
      toInt(raw && raw.C, 0),
      toInt(raw && raw.S, 0),
      toInt(raw && raw.E, 0),
      toInt(raw && raw.R, 0),
      toInt(raw && raw.O, 0),
    ];
    if (includeGold) out.push(toInt(raw && raw.G, 0));
    return out;
  }

  function vectorFromCostObject(cost, includeGold = false) {
    const out = [
      toInt(cost && cost.white, 0),
      toInt(cost && cost.blue, 0),
      toInt(cost && cost.green, 0),
      toInt(cost && cost.red, 0),
      toInt(cost && cost.black, 0),
    ];
    if (includeGold) out.push(toInt(cost && cost.gold, 0));
    return out;
  }

  function normalizeBgaCardId(cardLike) {
    if (cardLike == null) return "";
    if (typeof cardLike === "string" || typeof cardLike === "number") return String(cardLike);
    return String(cardLike.type || cardLike.card_id || cardLike.id || cardLike.cardId || "");
  }

  function activeExpansionWarnings(gd) {
    const warnings = [];
    const hasOrient = [1, 2, 3].some((tier) => {
      const row = gd && gd[`orient_row_${tier}`];
      return bgaCardEntries(row && row.cards).length > 0;
    });
    const hasCities = bgaCardEntries(gd && gd.market && gd.market.cities).length > 0 ||
      bgaCardEntries(gd && gd.cities).length > 0;
    const hasTradings = bgaCardEntries(gd && gd.market && gd.market.tradings).length > 0 ||
      bgaCardEntries(gd && gd.tradings).length > 0;
    const hasStrongholds = gd && gd.strongholds && Object.values(gd.strongholds).some((byPlayer) =>
      byPlayer && typeof byPlayer === "object" && Object.keys(byPlayer).length > 0);
    if (hasOrient) warnings.push("Orient cards are present; DinoBoard base model will only analyze base-market state.");
    if (hasCities) warnings.push("Cities are present; DinoBoard base model does not model Cities.");
    if (hasTradings) warnings.push("Trading posts are present; DinoBoard base model does not model Trading Posts.");
    if (hasStrongholds) warnings.push("Strongholds are present; DinoBoard base model does not model Strongholds.");
    return warnings;
  }

  function playerOrder(gd) {
    const rawOrder = Array.isArray(gd && gd.playerorder) ? gd.playerorder : [];
    const ids = rawOrder.map((id) => String(id)).filter((id) => gd.players && gd.players[id]);
    if (ids.length) return ids;
    return Object.keys(gd && gd.players || {});
  }

  function bonusesFromPlayedCards(player, carddb) {
    const bonuses = [0, 0, 0, 0, 0];
    let count = 0;
    for (const entry of bgaCardEntries(player && player.cards_played)) {
      const id = normalizeBgaCardId(entry);
      const record = carddb && carddb[id];
      const color = Number(record && record.type);
      if (Number.isFinite(color) && color >= 0 && color < 5) {
        bonuses[color] += Math.max(1, toInt(record && record.nbBonus, 1));
        count += 1;
      }
    }
    return {bonuses, count};
  }

  function reservedFromPlayer(player, carddb) {
    const visible = bgaCardEntries(player && (player.cards_reserved || player.reserved || player.cards_stock));
    const out = visible.slice(0, 3).map((entry) => {
      const id = normalizeBgaCardId(entry);
      const record = carddb && carddb[id];
      return {
        bga_id: id || null,
        visible: Boolean(id),
        tier: normalizeBgaLevel(record && record.lvl),
        points: toInt(record && record.points, 0),
        bonus_color: BGA_TYPE_COLORS[String(record && record.type)] || null,
        cost: vectorFromCostObject(countBgaCostLetters(record && record.cost)),
      };
    });
    const hiddenByTier = player && player.nb_cards_hidden && typeof player.nb_cards_hidden === "object"
      ? player.nb_cards_hidden
      : {};
    for (const tierKey of ["1", "2", "3"]) {
      for (let i = 0; i < toInt(hiddenByTier[tierKey], 0) && out.length < 3; i += 1) {
        out.push({bga_id: null, visible: false, tier: Number(tierKey), points: 0, bonus_color: null, cost: [0, 0, 0, 0, 0]});
      }
    }
    return out.slice(0, 3);
  }

  function buildDinoBoardSnapshot(domCards) {
    const gd = getGameDatas();
    if (!gd || !gd.players || !gd.market) return null;
    const order = playerOrder(gd);
    const warnings = activeExpansionWarnings(gd);
    if (order.length < 2 || order.length > 4) {
      warnings.push(`DinoBoard Splendor expects 2-4 players; BGA exposed ${order.length}.`);
    }
    const playerIndex = new Map(order.map((id, index) => [String(id), index]));
    const activeBgaId = String(gd.gamestate && gd.gamestate.active_player || gd.active_player || order[0] || "");
    const firstBgaId = String(gd.first_player || order[0] || "");
    const gamestateName = String(gd.gamestate && gd.gamestate.name || "").toLowerCase();
    const stage = gamestateName.includes("discard") || gamestateName.includes("return")
      ? 1
      : gamestateName.includes("noble")
        ? 2
        : 0;
    const carddb = bgaCardDb() || {};
    const cardByBgaId = new Map((domCards || []).map((card) => [String(card.card_id), card]));
    const players = order.map((id, index) => {
      const player = gd.players[String(id)] || {};
      const played = bonusesFromPlayedCards(player, carddb);
      return {
        index,
        bga_id: String(id),
        tokens: vectorFromBgaColorCounts(player.coins || {}, true),
        bonuses: played.bonuses,
        points: toInt(player.score, 0),
        cards_count: played.count,
        nobles_count: bgaCardEntries(player.nobles_played).length,
        reserved: reservedFromPlayer(player, carddb),
      };
    });
    const market = [1, 2, 3].map((tier) => sortedBgaMarketCards(tier).slice(0, 4).map((entry, slot) => {
      const id = String(entry.type || entry.id || "");
      const record = normalizeBgaCardRecord(bgaCardRecord(id));
      const domCard = cardByBgaId.get(id);
      return {
        tier: tier - 1,
        slot,
        bga_id: id,
        client_id: domCard && domCard.client_id || null,
        action_ids: {
          buy: (tier - 1) * 4 + slot,
          reserve: 12 + (tier - 1) * 4 + slot,
        },
        points: record ? record.points : 0,
        bonus_color: record && record.bonus_color || null,
        cost: record ? vectorFromCostObject(record.cost) : [0, 0, 0, 0, 0],
      };
    }));
    const deckSizes = [1, 2, 3].map((tier) => {
      const row = gd.market && gd.market[`row_${tier}`];
      return toInt(row && row.count, 0);
    });
    const nobleDb = gd.nobledb || {};
    const nobles = bgaCardEntries(gd.market && gd.market.nobles || gd.nobles)
      .slice(0, order.length + 1)
      .map((entry, slot) => {
        const id = String(entry.type || entry.id || "");
        const record = nobleDb[id] || {};
        return {
          slot,
          bga_id: id,
          points: toInt(record.points, 3),
          requirements: vectorFromCostObject(countBgaCostLetters(record.cost)),
        };
      });
    const active = playerIndex.has(activeBgaId) ? playerIndex.get(activeBgaId) : 0;
    const activeTokens = players[active] ? players[active].tokens.reduce((sum, n) => sum + n, 0) : 0;
    const pendingReturns = stage === 1 ? Math.max(0, activeTokens - 10) : 0;
    return {
      schema: "gemhud-dinoboard-splendor-public-snapshot-v1",
      supported: warnings.length === 0 && order.length >= 2 && order.length <= 4,
      warnings,
      game_id: `splendor_${Math.max(2, Math.min(4, order.length))}p`,
      num_players: order.length,
      current_player: active,
      first_player: playerIndex.has(firstBgaId) ? playerIndex.get(firstBgaId) : 0,
      plies: Math.max(0, toInt(gd.gamestate && gd.gamestate.args && gd.gamestate.args.turn, toInt(gd.roundnumber, 1) - 1)),
      stage,
      pending_returns: pendingReturns,
      bank: vectorFromBgaColorCounts(gd.pool || {}, true),
      players,
      market,
      deck_sizes: deckSizes,
      nobles,
    };
  }

  function hullClassNumber(el, prefix) {
    const cls = String(el && el.className || "");
    const m = cls.match(new RegExp(`${prefix}-(\\d+)`));
    return m ? toInt(m[1], null) : null;
  }

  function hullCostVectorFromElement(el) {
    const out = [0, 0, 0, 0, 0];
    el.querySelectorAll(".ccbs-circle").forEach((node) => {
      const color = hullClassNumber(node, "ccbs-color");
      if (color == null || color < 0 || color > 4) return;
      out[color] = toInt(node.textContent, 0);
    });
    return out;
  }

  function hullCostObject(cost) {
    const out = {};
    HULL_BONUS_COLORS.forEach((color, index) => {
      if (cost[index] > 0) out[color] = cost[index];
    });
    return out;
  }

  function hullVisibleCardNodes() {
    return Array.from(document.querySelectorAll("[id^='ccbs-card-']")).filter((el) =>
      /^ccbs-card-[012]-[0-3]$/.test(el.id || "") &&
      el.querySelector(".ccbs-card") &&
      !el.querySelector(".ccbs-empty")
    );
  }

  function hullMatchCardId({tierIndex, bonus, points, cost, image}, used) {
    let fallback = null;
    for (let i = 0; i < 90; i += 1) {
      if (HULL_CARD_TIER[i] !== tierIndex) continue;
      if (HULL_CARD_BONUS[i] !== bonus) continue;
      if (HULL_CARD_POINTS[i] !== points) continue;
      if (HULL_CARD_IMAGE[i] !== image) continue;
      if (!HULL_CARD_COSTS[i].every((n, idx) => n === cost[idx])) continue;
      if (!used.has(i)) {
        used.add(i);
        return i + 1;
      }
      fallback = i + 1;
    }
    for (let i = 0; i < 90; i += 1) {
      if (HULL_CARD_TIER[i] !== tierIndex) continue;
      if (HULL_CARD_BONUS[i] !== bonus) continue;
      if (HULL_CARD_POINTS[i] !== points) continue;
      if (!HULL_CARD_COSTS[i].every((n, idx) => n === cost[idx])) continue;
      if (!used.has(i)) {
        used.add(i);
        return i + 1;
      }
      fallback = fallback || i + 1;
    }
    return fallback;
  }

  function extractHullQinCards() {
    const used = new Set();
    const cards = [];
    lastCardElements = new Map();
    lastCardMeta = new Map();
    for (const el of hullVisibleCardNodes()) {
      const match = (el.id || "").match(/^ccbs-card-([012])-([0-3])$/);
      if (!match) continue;
      const tierIndex = toInt(match[1], 0);
      const slot = toInt(match[2], 0);
      const cardEl = el.querySelector(".ccbs-card");
      const bonus = hullClassNumber(cardEl, "ccbs-type");
      const image = hullClassNumber(cardEl, "ccbs-img");
      if (bonus == null || bonus < 0 || bonus > 4) continue;
      const scoreNode = el.querySelector(".ccbs-score.left-0\\.5");
      const points = toInt(scoreNode && scoreNode.textContent, 0);
      const costVector = hullCostVectorFromElement(el);
      const id = hullMatchCardId({tierIndex, bonus, points, cost: costVector, image}, used);
      const marketIndex = tierIndex * 4 + slot;
      const clientId = el.getAttribute(CARD_MARK) || `hull:${tierIndex}:${slot}:${id || "unknown"}`;
      el.setAttribute(CARD_MARK, clientId);
      const card = {
        client_id: clientId,
        source: "hullqin-dom",
        card_id: id,
        tier: tierIndex + 1,
        points,
        bonus_color: HULL_BONUS_COLORS[bonus],
        cost: hullCostObject(costVector),
        location: "market",
        market_index: marketIndex,
        tier_index: tierIndex,
        slot_index: slot,
        buy_action_id: marketIndex,
        reserve_action_id: 12 + marketIndex,
        raw_text: readText(el).slice(0, 500),
        raw_hint: `ccbs tier=${tierIndex + 1} slot=${slot} img=${image}`,
      };
      cards.push(card);
      lastCardElements.set(clientId, el);
      lastCardMeta.set(clientId, card);
    }
    return cards;
  }

  function hullParseBankTokens() {
    const out = [0, 0, 0, 0, 0, 0];
    const nodes = Array.from(document.querySelectorAll(".ccbs-circle.scale-125"));
    for (const node of nodes) {
      if (node.closest("#gemhud-panel")) continue;
      const color = hullClassNumber(node, "ccbs-color");
      if (color == null || color < 0 || color > 5) continue;
      const n = toInt(node.textContent, null);
      if (n == null) continue;
      out[color] = n;
    }
    return out;
  }

  function hullParseDeckSizes() {
    return [0, 1, 2].map((tierIndex) => {
      const node = document.querySelector(`#ccbs-card-${tierIndex} .ccbs-left-count`);
      return toInt(node && node.textContent, 0);
    });
  }

  function hullParseNobles() {
    return Array.from(document.querySelectorAll("[id^='ccbs-noble-']")).filter((el) =>
      /^ccbs-noble-\d+$/.test(el.id || "") && !el.querySelector(".ccbs-empty")
    ).map((el, slot) => {
      const nobleEl = el.querySelector(".ccbs-noble");
      const nobleIdx = hullClassNumber(nobleEl, "ccbs-noble");
      const requirements = [0, 0, 0, 0, 0];
      el.querySelectorAll(".ccbs-rect").forEach((node) => {
        const color = hullClassNumber(node, "ccbs-color");
        if (color == null || color < 0 || color > 4) return;
        requirements[color] = toInt(node.textContent, 0);
      });
      return {
        slot,
        bga_id: nobleIdx == null ? null : String(nobleIdx + 1),
        points: 3,
        requirements,
      };
    }).filter((noble) => noble.requirements.some(Boolean));
  }

  function hullCurrentPlayerIndex() {
    const text = document.body && document.body.innerText || "";
    const m = text.match(/等待(?:玩家|你)(\d+)?(?:丢弃多余宝石|操作)/);
    if (!m) return 0;
    return Math.max(0, toInt(m[1], 1) - 1);
  }

  function hullParsePlayers() {
    const seats = Array.from(document.querySelectorAll("[id^='userseat']")).filter((el) => /^userseat\d+$/.test(el.id || ""));
    return seats.map((seat, index) => {
      const row = seat.parentElement || seat;
      const bonuses = [0, 0, 0, 0, 0];
      const tokens = [0, 0, 0, 0, 0, 0];
      for (let color = 0; color < 5; color += 1) {
        const rect = row.querySelector(`.ccbs-rect.ccbs-color-${color}.mx-auto`);
        bonuses[color] = toInt(rect && rect.textContent, 0);
        const gem = row.querySelector(`.ccbs-circle.ccbs-color-${color}.scale-75`);
        tokens[color] = toInt(gem && gem.textContent, 0);
      }
      const gold = row.querySelector(".ccbs-circle.ccbs-color-5.scale-75");
      tokens[5] = toInt(gold && gold.textContent, 0);
      const scoreText = row.innerText || "";
      const scoreMatch = scoreText.match(/(\d+)分/);
      const reserved = Array.from(row.querySelectorAll(".ccbs-card-wrapper.ccbs-small .ccbs-card.ccbs-type-5")).slice(0, 3).map((node) => {
        const tierIndex = hullClassNumber(node, "ccbs-img");
        return {
          bga_id: null,
          visible: false,
          tier: tierIndex == null ? null : tierIndex + 1,
          points: 0,
          bonus_color: null,
          cost: [0, 0, 0, 0, 0],
        };
      });
      return {
        index,
        bga_id: String(index + 1),
        tokens,
        bonuses,
        points: scoreMatch ? toInt(scoreMatch[1], 0) : 0,
        cards_count: bonuses.reduce((sum, n) => sum + n, 0),
        nobles_count: row.querySelectorAll(".ccbs-noble-wrapper.ccbs-small").length,
        reserved,
      };
    });
  }

  function buildHullQinSnapshot(cards) {
    const players = hullParsePlayers();
    const market = [0, 1, 2].map((tierIndex) => cards
      .filter((card) => card.tier_index === tierIndex)
      .sort((a, b) => a.slot_index - b.slot_index)
      .map((card) => ({
        tier: tierIndex,
        slot: card.slot_index,
        bga_id: card.card_id == null ? null : String(card.card_id),
        client_id: card.client_id,
        action_ids: {buy: card.buy_action_id, reserve: card.reserve_action_id},
        points: card.points || 0,
        bonus_color: card.bonus_color,
        cost: vectorFromCostObject(card.cost),
      })));
    const warnings = [];
    if (players.length < 2 || players.length > 4) {
      warnings.push(`HullQin ccbs player count is ${players.length}; DinoBoard expects 2-4 players.`);
    }
    return {
      schema: "gemhud-dinoboard-splendor-public-snapshot-v1",
      source: "hullqin-ccbs",
      supported: players.length >= 2 && players.length <= 4,
      warnings,
      game_id: `splendor_${Math.max(2, Math.min(4, players.length || 2))}p`,
      num_players: players.length || 2,
      current_player: Math.min(hullCurrentPlayerIndex(), Math.max(0, players.length - 1)),
      first_player: 0,
      plies: 0,
      stage: 0,
      pending_returns: 0,
      bank: hullParseBankTokens(),
      players,
      market,
      deck_sizes: hullParseDeckSizes(),
      nobles: hullParseNobles().slice(0, (players.length || 2) + 1),
    };
  }

  function buildHullQinPayload() {
    const cards = extractHullQinCards();
    const snapshot = buildHullQinSnapshot(cards);
    return {
      source: "hullqin-ccbs",
      game: "splendor_base",
      version: VERSION,
      url: location.href,
      generated_at: new Date().toISOString(),
      capabilities: {
        values_only: true,
        automation: false,
        base_splendor_only: true,
        action_recommendation: true,
      },
      cards,
      dom_card_count: cards.length,
      carddb_card_count: cards.length,
      dinoboard_snapshot: snapshot,
      public_context: {
        source: "hullqin-ccbs-dom",
        room: location.pathname.split("/").filter(Boolean).pop() || null,
      },
    };
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
    const nodes = Array.from(document.querySelectorAll(CARD_SELECTOR))
      .map((el) => currentBgaCardRoot(el) || el);
    const dedup = new Set();
    const cards = [];
    lastCardElements = new Map();
    lastCardMeta = new Map();

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
      const bgaCard = normalizeBgaCardRecord(bgaCardRecord(id));
      const bgaPosition = bgaMarketPosition(id);
      const parsedCost = parseCost(source);
      const clientId = el.getAttribute(CARD_MARK) || `dom:${id}:${Math.round(rect.left)}:${Math.round(rect.top)}`;
      if (dedup.has(clientId)) return;
      dedup.add(clientId);
      el.setAttribute(CARD_MARK, clientId);
      lastCardElements.set(clientId, el);
      const card = {
        client_id: clientId,
        source: "dom",
        card_id: id,
        tier: bgaPosition && bgaPosition.tier || bgaCard && bgaCard.tier || parseTier(source) || bgaTierFromElement(el),
        points: bgaCard ? bgaCard.points : parsePoints(source),
        bonus_color: bgaCard && bgaCard.bonus_color || parseBonusColor(source),
        cost: bgaCard && Object.keys(bgaCard.cost).length ? bgaCard.cost : parsedCost,
        location: bgaPosition ? "market" : inferLocation(el),
        market_index: bgaPosition && bgaPosition.market_index,
        tier_index: bgaPosition && bgaPosition.tier_index,
        slot_index: bgaPosition && bgaPosition.slot_index,
        buy_action_id: bgaPosition && bgaPosition.market_index,
        reserve_action_id: bgaPosition && 12 + bgaPosition.market_index,
        raw_text: readText(el).slice(0, 500),
        raw_hint: source.slice(0, 1000),
        bga_carddb_found: Boolean(bgaCard),
        bga_carddb: bgaCard && bgaCard.bga_carddb || null,
        ability_labels: bgaCard && bgaCard.ability_labels || [],
        card_cost: bgaCard && bgaCard.card_cost || {},
      };
      cards.push(card);
      lastCardMeta.set(clientId, card);
    });
    annotateBaseSplendorActionIds(cards);
    return cards;
  }

  function annotateBaseSplendorActionIds(cards) {
    const marketCards = cards.filter((card) => card.location !== "reserved" && card.location !== "noble");
    const tierSlots = {};
    marketCards.forEach((card, index) => {
      if (Number.isFinite(Number(card.market_index))) return;
      let tierIndex = Number.isFinite(Number(card.tier)) ? Number(card.tier) - 1 : Math.floor(index / 4);
      if (!Number.isFinite(tierIndex)) tierIndex = Math.floor(index / 4);
      tierIndex = Math.max(0, Math.min(2, tierIndex));
      const key = String(tierIndex);
      const slot = tierSlots[key] || 0;
      tierSlots[key] = slot + 1;
      if (slot >= 4) return;
      const marketIndex = tierIndex * 4 + slot;
      card.market_index = marketIndex;
      card.tier_index = tierIndex;
      card.slot_index = slot;
      card.buy_action_id = marketIndex;
      card.reserve_action_id = 12 + marketIndex;
    });
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
    if (isHullQinCcbsPage()) return buildHullQinPayload();
    const domCards = extractDomCards();
    const dataCards = domCards.length ? [] : extractDataCards().slice(0, 32);
    const carddbCount = domCards.filter((card) => card.bga_carddb_found).length;
    const dinoboardSnapshot = buildDinoBoardSnapshot(domCards);
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
      carddb_card_count: carddbCount,
      dinoboard_snapshot: dinoboardSnapshot,
      public_context: sanitizeValue(getGameDatas(), 0),
    };
  }

  function postJson(url, payload) {
    return new Promise((resolve, reject) => {
      const body = JSON.stringify(payload);
      let settled = false;
      const finish = (fn, value) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        fn(value);
      };
      const timer = window.setTimeout(() => {
        finish(reject, new Error("Advisor request timed out"));
      }, 12000);
      if (typeof GM_xmlhttpRequest === "function") {
        GM_xmlhttpRequest({
          method: "POST",
          url,
          data: body,
          headers: {"Content-Type": "application/json"},
          timeout: 10000,
          onload: (res) => {
            try {
              finish(resolve, JSON.parse(res.responseText || "{}"));
            } catch (err) {
              finish(reject, new Error(`Advisor returned invalid JSON: ${err.message}`));
            }
          },
          onerror: () => finish(reject, new Error("Advisor request failed")),
          onabort: () => finish(reject, new Error("Advisor request aborted")),
          ontimeout: () => finish(reject, new Error("Advisor request timed out")),
        });
        return;
      }
      fetch(url, {
        method: "POST",
        headers: {"Content-Type": "application/json"},
        body,
      }).then((r) => r.json()).then((value) => finish(resolve, value), (err) => finish(reject, err));
    });
  }

  function setStatus(text) {
    const el = document.querySelector("#gemhud-status");
    if (el) el.textContent = text;
  }

  function renderBadges(response) {
    document.querySelectorAll(`.${BADGE_CLASS}, .${META_CLASS}`).forEach((el) => el.remove());
    const values = Array.isArray(response && response.cards) ? response.cards : [];
    let rendered = 0;
    for (const item of values) {
      const el = lastCardElements.get(item.client_id);
      const meta = lastCardMeta.get(item.client_id);
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
      badge.title = [
        `GemHUD value ${pct}/100`,
        `Method: ${item.method || "local advisor"}`,
        verboseCardSummary(meta),
        ...(Array.isArray(item.reasons) ? item.reasons : []),
        "Values only; no automation.",
      ].filter(Boolean).join("\n");
      el.appendChild(badge);
      if (meta) {
        const metaBadge = document.createElement("div");
        metaBadge.className = META_CLASS;
        metaBadge.textContent = compactCardSummary(meta);
        metaBadge.title = verboseCardSummary(meta);
        el.appendChild(metaBadge);
      }
      rendered += 1;
    }
    return rendered;
  }

  function renderRecommendation(response) {
    const el = document.querySelector("#gemhud-reco");
    if (!el) return;
    const recommendation = response && response.recommendation;
    if (!recommendation || !recommendation.label) {
      el.textContent = "建议: -";
      el.title = "";
      return;
    }
    const value = Number(recommendation.value);
    const suffix = Number.isFinite(value) ? ` (${Math.round(value * 100)})` : "";
    el.textContent = `建议: ${recommendation.label}${suffix}`;
    el.title = [
      recommendation.method || "advisor",
      ...(Array.isArray(recommendation.reasons) ? recommendation.reasons : []),
      "Values only; no automatic moves.",
    ].filter(Boolean).join("\n");
  }

  async function runScan(reason) {
    if (!enabled) {
      setStatus("Disabled");
      return;
    }
    if (!isSplendorPage()) {
      setStatus("Waiting for Splendor page");
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

    setStatus(`Sending ${payload.dom_card_count} cards, ${payload.carddb_card_count || 0} from carddb, snapshot ${payload.dinoboard_snapshot ? "mapped" : "missing"}`);
    try {
      const response = await postJson(endpoint(), payload);
      if (seq !== scanSeq) return;
      const count = renderBadges(response);
      renderRecommendation(response);
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
        <div id="gemhud-reco" class="gemhud-reco">建议: -</div>
        <div class="gemhud-note">Base Splendor values and action suggestions only. No automatic moves.</div>
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
