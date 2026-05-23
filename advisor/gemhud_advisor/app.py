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
    public_context: dict[str, Any] | None = None


class CardValue(BaseModel):
    client_id: str
    value: float
    confidence: float
    method: str
    label: str
    reasons: list[str] = Field(default_factory=list)


class AnalyzeResponse(BaseModel):
    ok: bool = True
    engine: str
    version: str
    game: str
    scope: str
    cards: list[CardValue]
    warnings: list[str] = Field(default_factory=list)


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

    values = [score_card(card) for card in req.cards if card.source != "gamedatas" or card.client_id]
    return AnalyzeResponse(
        engine="gemhud-card-value-v0",
        version=__version__,
        game="splendor_base",
        scope="public visible cards; values only; no action automation",
        cards=values,
        warnings=warnings,
    )


def score_card(card: CardInput) -> CardValue:
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

    value = clamp(score, 0.0, 1.0)
    confidence = card_confidence(card)
    label = value_label(value)
    return CardValue(
        client_id=card.client_id,
        value=value,
        confidence=confidence,
        method="public-card-heuristic-v0",
        label=label,
        reasons=reasons[:5],
    )


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
