"""GemHUD local advisor.

The service intentionally exposes only value-analysis endpoints. It does not
choose, click, submit, or automate BGA moves.
"""
from __future__ import annotations

from math import isfinite
from typing import Any, Literal

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field, field_validator

from . import __version__


BASE_SPLENDOR_GAME_IDS = {"splendor", "splendor_base", "base_splendor"}
COLORS = ("white", "blue", "green", "red", "black")


class CardInput(BaseModel):
    client_id: str = Field(..., min_length=1)
    source: str | None = None
    card_id: str | int | None = None
    tier: int | None = Field(None, ge=1, le=3)
    points: int | None = Field(None, ge=0, le=10)
    bonus_color: str | None = None
    cost: dict[str, int] = Field(default_factory=dict)
    location: str | None = None
    buy_action_id: int | None = None
    reserve_action_id: int | None = None
    market_index: int | None = None
    raw_text: str | None = None
    raw_hint: str | None = None

    @field_validator("bonus_color")
    @classmethod
    def normalize_bonus(cls, value: str | None) -> str | None:
      if value is None:
          return None
      value = value.strip().lower()
      return value if value in COLORS else value or None

    @field_validator("cost")
    @classmethod
    def normalize_cost(cls, value: dict[str, Any]) -> dict[str, int]:
        out: dict[str, int] = {}
        for key, raw in value.items():
            color = str(key).strip().lower()
            if color not in COLORS:
                continue
            try:
                n = int(raw)
            except (TypeError, ValueError):
                continue
            if n >= 0:
                out[color] = min(n, 12)
        return out


class AnalyzeRequest(BaseModel):
    source: Literal["bga"] | str = "bga"
    game: str = "splendor_base"
    version: str | None = None
    url: str | None = None
    generated_at: str | None = None
    capabilities: dict[str, Any] = Field(default_factory=dict)
    cards: list[CardInput] = Field(default_factory=list, max_length=256)
    dom_card_count: int | None = None
    dinoboard_snapshot: dict[str, Any] | None = None
    public_context: dict[str, Any] | None = None


class CardValue(BaseModel):
    client_id: str
    value: float
    confidence: float
    method: str
    label: str
    reasons: list[str] = Field(default_factory=list)


class ActionRecommendation(BaseModel):
    label: str
    action_id: int | None = None
    value: float | None = None
    confidence: float
    method: str
    reasons: list[str] = Field(default_factory=list)


class AnalyzeResponse(BaseModel):
    ok: bool = True
    engine: str
    version: str
    game: str
    scope: str
    cards: list[CardValue]
    warnings: list[str] = Field(default_factory=list)
    recommendation: ActionRecommendation | None = None


app = FastAPI(
    title="GemHUD Advisor",
    version=__version__,
    description="Local base Splendor public-card value analysis for GemHUD.",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "https://boardgamearena.com",
        "https://en.boardgamearena.com",
        "https://studio.boardgamearena.com",
        "https://game.hullqin.cn",
    ],
    allow_methods=["GET", "POST"],
    allow_headers=["Content-Type"],
)


@app.get("/health")
def health() -> dict[str, Any]:
    return {
        "ok": True,
        "service": "gemhud-advisor",
        "version": __version__,
        "scope": "base Splendor public card value analysis only",
        "automation": False,
    }


@app.post("/analyze", response_model=AnalyzeResponse)
def analyze(req: AnalyzeRequest) -> AnalyzeResponse:
    game = req.game.strip().lower()
    if game not in BASE_SPLENDOR_GAME_IDS:
        raise HTTPException(
            status_code=400,
            detail=(
                "GemHUD currently supports base Splendor only. "
                "Orient, Strongholds, Cities, and Sun Never Sets variants are not enabled."
            ),
        )
    if req.capabilities.get("automation") is True:
        raise HTTPException(
            status_code=400,
            detail="GemHUD advisor accepts values-only clients and does not support automation.",
        )

    warnings: list[str] = []
    if not req.cards:
        warnings.append("No cards were detected in the request.")

    if req.dinoboard_snapshot:
        append_snapshot_warnings(req.dinoboard_snapshot, warnings)
    values = [
        score_card(card, req.dinoboard_snapshot)
        for card in req.cards
        if card.source != "gamedatas" or card.client_id
    ]
    return AnalyzeResponse(
        engine="gemhud-card-value-v0",
        version=__version__,
        game="splendor_base",
        scope="public visible cards; values only; no action automation",
        cards=values,
        warnings=warnings,
        recommendation=recommend_action(req.dinoboard_snapshot, req.cards),
    )


def append_snapshot_warnings(snapshot: dict[str, Any], warnings: list[str]) -> None:
    if snapshot.get("supported") is False:
        warnings.append(
            "Mapped BGA state is not base-Splendor-only; expansion rules are outside the current DinoBoard base model."
        )
    for item in snapshot.get("warnings") or []:
        if isinstance(item, str):
            warnings.append(f"BGA snapshot: {item}")


def score_card(card: CardInput, snapshot: dict[str, Any] | None = None) -> CardValue:
    """Score one visible base Splendor card on a 0..1 practice scale.

    This first advisor intentionally uses only public card features available
    from the BGA page. A future DinoBoard-backed adapter can replace this
    method with MCTS root action values once the full BGA base-state mapping is
    validated.
    """

    tier = clamp_number(card.tier, 1, 3, default=2)
    points = clamp_number(card.points, 0, 10, default=0)
    cost_total = sum(max(0, int(v)) for v in card.cost.values())
    color_count = sum(1 for v in card.cost.values() if int(v) > 0)

    reasons: list[str] = []
    if card.points is not None:
        reasons.append(f"{card.points} prestige")
    if card.tier is not None:
        reasons.append(f"tier {card.tier}")
    if card.bonus_color:
        reasons.append(f"{card.bonus_color} bonus")
    if cost_total:
        reasons.append(f"cost {cost_total}")

    prestige_efficiency = points / max(1.0, cost_total)
    low_cost_bonus = max(0.0, (7.0 - cost_total) / 7.0) * 0.12
    color_focus_bonus = max(0.0, (4.0 - color_count) / 4.0) * 0.08
    tier_prior = {1: 0.18, 2: 0.32, 3: 0.46}.get(tier, 0.28)
    score = tier_prior + points * 0.095 + prestige_efficiency * 0.22 + low_cost_bonus + color_focus_bonus

    if points == 0 and tier == 1:
        score += 0.08
        reasons.append("early engine card")
    if points >= 4:
        score += 0.08
        reasons.append("high prestige")
    if cost_total == 0:
        score *= 0.7
    if card.location == "reserved":
        score *= 0.92

    method = "public-card-heuristic-v0"
    confidence = card_confidence(card)
    if snapshot:
        adjustment, snapshot_reasons = state_adjustment(card, snapshot)
        score += adjustment
        reasons.extend(snapshot_reasons)
        method = "bga-state-aware-heuristic-v1"
        confidence = max(confidence, 0.9)

    value = clamp(score, 0.0, 1.0)
    label = value_label(value)
    return CardValue(
        client_id=card.client_id,
        value=value,
        confidence=confidence,
        method=method,
        label=label,
        reasons=reasons[:6],
    )


def state_adjustment(card: CardInput, snapshot: dict[str, Any]) -> tuple[float, list[str]]:
    player = current_snapshot_player(snapshot)
    if not player:
        return 0.0, ["snapshot missing active player"]
    gems = int_list(player.get("tokens"), 6)
    bonuses = int_list(player.get("bonuses"), 5)
    cost = [int(card.cost.get(color, 0)) for color in COLORS]
    deficit, gold_used = payment_deficit(cost, bonuses, gems)
    adjustment = 0.0
    reasons: list[str] = []
    if deficit == 0:
        adjustment += 0.16
        reasons.append(f"buyable now using {gold_used} gold" if gold_used else "buyable now")
    elif deficit <= 2:
        adjustment += 0.06
        reasons.append(f"{deficit} token short")
    else:
        adjustment -= min(0.18, deficit * 0.035)
        reasons.append(f"{deficit} tokens short")
    if opponent_can_buy(card, snapshot):
        adjustment += 0.04
        reasons.append("opponent can buy")
    if advances_visible_noble(card, snapshot, bonuses):
        adjustment += 0.05
        reasons.append("helps visible noble")
    return adjustment, reasons


def current_snapshot_player(snapshot: dict[str, Any]) -> dict[str, Any] | None:
    players = snapshot.get("players")
    if not isinstance(players, list):
        return None
    current = int(snapshot.get("current_player") or 0)
    if current < 0 or current >= len(players):
        return None
    player = players[current]
    return player if isinstance(player, dict) else None


def int_list(value: Any, length: int) -> list[int]:
    out = [0] * length
    if isinstance(value, list):
        for idx, raw in enumerate(value[:length]):
            try:
                out[idx] = max(0, int(raw))
            except (TypeError, ValueError):
                out[idx] = 0
    return out


def payment_deficit(cost: list[int], bonuses: list[int], gems: list[int]) -> tuple[int, int]:
    deficit = 0
    gold_left = gems[5] if len(gems) > 5 else 0
    gold_used = 0
    for idx in range(len(COLORS)):
        need = max(0, cost[idx] - bonuses[idx])
        short = max(0, need - gems[idx])
        covered = min(short, gold_left)
        gold_left -= covered
        gold_used += covered
        deficit += short - covered
    return deficit, gold_used


def opponent_can_buy(card: CardInput, snapshot: dict[str, Any]) -> bool:
    players = snapshot.get("players")
    if not isinstance(players, list):
        return False
    current = int(snapshot.get("current_player") or 0)
    cost = [int(card.cost.get(color, 0)) for color in COLORS]
    for idx, player in enumerate(players):
        if idx == current or not isinstance(player, dict):
            continue
        if payment_deficit(cost, int_list(player.get("bonuses"), 5), int_list(player.get("tokens"), 6))[0] == 0:
            return True
    return False


def advances_visible_noble(card: CardInput, snapshot: dict[str, Any], bonuses: list[int]) -> bool:
    if card.bonus_color not in COLORS:
        return False
    color = COLORS.index(card.bonus_color)
    nobles = snapshot.get("nobles")
    if not isinstance(nobles, list):
        return False
    for noble in nobles:
        if not isinstance(noble, dict):
            continue
        req = int_list(noble.get("requirements"), 5)
        if req[color] <= bonuses[color]:
            continue
        missing = sum(max(0, needed - bonuses[idx]) for idx, needed in enumerate(req))
        if missing <= 5:
            return True
    return False


def recommend_action(snapshot: dict[str, Any] | None, cards: list[CardInput]) -> ActionRecommendation | None:
    if not snapshot:
        return None
    player = current_snapshot_player(snapshot)
    if not player:
        return None
    gems = int_list(player.get("tokens"), 6)
    bonuses = int_list(player.get("bonuses"), 5)
    best_buy: tuple[CardInput, float] | None = None
    best_target: tuple[CardInput, float, int] | None = None
    for card in cards:
        cost = [int(card.cost.get(color, 0)) for color in COLORS]
        deficit, _ = payment_deficit(cost, bonuses, gems)
        value = score_card(card, snapshot).value
        if deficit == 0 and (best_buy is None or value > best_buy[1]):
            best_buy = (card, value)
        if deficit > 0 and (
            best_target is None or value - deficit * 0.04 > best_target[1] - best_target[2] * 0.04
        ):
            best_target = (card, value, deficit)
    if best_buy:
        card, value = best_buy
        return ActionRecommendation(
            label=f"购买 {short_card_label(card)}",
            action_id=getattr(card, "buy_action_id", None),
            value=value,
            confidence=0.72,
            method="state-aware-heuristic-v1",
            reasons=["card is affordable now", short_card_reason(card), "values only; no automation"],
        )
    if best_target:
        card, value, deficit = best_target
        take = suggested_gems_for_card(card, bonuses, gems, int_list(snapshot.get("bank"), 6))
        if take:
            return ActionRecommendation(
                label=f"拿宝石 {' '.join(take)}",
                value=clamp(value - deficit * 0.03, 0.0, 1.0),
                confidence=0.62,
                method="state-aware-heuristic-v1",
                reasons=[f"toward {short_card_label(card)}", f"{deficit} tokens short", "values only; no automation"],
            )
    return ActionRecommendation(
        label="放弃或调整目标",
        value=0.2,
        confidence=0.35,
        method="state-aware-heuristic-v1",
        reasons=["no clearly useful public action found"],
    )


def suggested_gems_for_card(card: CardInput, bonuses: list[int], gems: list[int], bank: list[int]) -> list[str]:
    needed: list[tuple[int, int]] = []
    for idx, color in enumerate(COLORS):
        need = max(0, int(card.cost.get(color, 0)) - bonuses[idx] - gems[idx])
        if need > 0 and bank[idx] > 0:
            needed.append((idx, need))
    needed.sort(key=lambda item: item[1], reverse=True)
    if needed and needed[0][1] >= 2 and bank[needed[0][0]] >= 4:
        return [color_short(needed[0][0]), color_short(needed[0][0])]
    return [color_short(idx) for idx, _ in needed[:3]]


def color_short(idx: int) -> str:
    return ("W", "U", "G", "R", "B")[idx] if 0 <= idx < 5 else "?"


def short_card_label(card: CardInput) -> str:
    color = color_short(COLORS.index(card.bonus_color)) if card.bonus_color in COLORS else "?"
    return f"T{card.tier or 0} {color} {card.points or 0}P"


def short_card_reason(card: CardInput) -> str:
    return f"{short_card_label(card)} cost {sum(int(v) for v in card.cost.values())}"


def clamp(value: float, lo: float, hi: float) -> float:
    if not isfinite(value):
        return lo
    return min(max(value, lo), hi)


def clamp_number(value: int | None, lo: int, hi: int, *, default: int) -> int:
    if value is None:
        return default
    return int(clamp(float(value), float(lo), float(hi)))


def card_confidence(card: CardInput) -> float:
    fields = [
        card.tier is not None,
        card.points is not None,
        card.bonus_color is not None,
        bool(card.cost),
    ]
    return clamp(0.2 + sum(0.2 for present in fields if present), 0.2, 1.0)


def value_label(value: float) -> str:
    if value >= 0.66:
        return "high"
    if value >= 0.33:
        return "medium"
    return "low"
