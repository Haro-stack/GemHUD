# Fair Play and Scope

GemHUD is designed as a local practice and study tool for visible BGA base
Splendor card valuation.

## Hard Limits

- No automatic move execution.
- No BGA button clicking.
- No action submission endpoints.
- No hidden deck inspection.
- No credential storage.
- No expansion support until the rule engine and converter are implemented.

The userscript sends public page-derived card data to a local advisor and
renders value badges. It does not return a "best move" command and does not
perform actions on behalf of the player.

## Supported Game Scope

Supported:

- Base Splendor on BGA.
- Public visible development cards.
- Local value badge rendering.

Not supported yet:

- Orient.
- Strongholds.
- Cities.
- Sun Never Sets combinations.
- Hidden deck order analysis.
- Live move automation.

## DinoBoard Reference

GemHUD is planned around the local AI work in
[Haro-stack/DinoBoard](https://github.com/Haro-stack/DinoBoard). The current
GemHUD API is intentionally narrow so a DinoBoard action-value adapter can be
added later without changing the Tampermonkey UI contract.
