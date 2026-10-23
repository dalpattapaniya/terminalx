//! Small, explicit token price table used only for local cost estimates.
//!
//! Prices are USD per million tokens. Subscription billing and provider
//! discounts are unknowable from transcripts, so callers always label these
//! values as estimates.

#[derive(Clone, Copy)]
struct ClaudePrice {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    long_context: Option<(u64, f64, f64, f64, f64)>,
}

#[derive(Clone, Copy)]
struct CodexPrice {
    input: f64,
    cached_input: f64,
    output: f64,
    long_context: Option<(u64, f64, f64, f64)>,
}

// Claude rows follow https://platform.claude.com/docs/en/about-claude/pricing
// (reviewed 2026-10-23). `cache_write` is the 5-minute rate.
const CLAUDE_PRICES: &[(&str, ClaudePrice)] = &[
    (
        "claude-fable-5-1",
        ClaudePrice {
            input: 10.0,
            output: 50.0,
            cache_read: 0.25,
            cache_write: 12.5,
            long_context: None,
        },
    ),
    (
        "claude-fable-5",
        ClaudePrice {
            input: 10.0,
            output: 50.0,
            cache_read: 1.0,
            cache_write: 12.5,
            long_context: None,
        },
    ),
    (
        "claude-opus-5-5",
        ClaudePrice {
            input: 4.0,
            output: 20.0,
            cache_read: 0.2,
            cache_write: 5.0,
            long_context: None,
        },
    ),
    (
        "claude-opus-5",
        ClaudePrice {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
            long_context: None,
        },
    ),
    (
        "claude-sonnet-5-5",
        ClaudePrice {
            input: 2.0,
            output: 10.0,
            cache_read: 0.1,
            cache_write: 2.5,
            long_context: None,
        },
    ),
    (
        "claude-sonnet-5",
        ClaudePrice {
            input: 2.0,
            output: 10.0,
            cache_read: 0.2,
            cache_write: 2.5,
            long_context: None,
        },
    ),
    (
        "claude-opus-4-8",
        ClaudePrice {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
            long_context: None,
        },
    ),
    (
        "claude-opus-4-7",
        ClaudePrice {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
            long_context: None,
        },
    ),
    (
        "claude-opus-4-6",
        ClaudePrice {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
            long_context: None,
        },
    ),
    (
        "claude-opus-4-5",
        ClaudePrice {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
            long_context: None,
        },
    ),
    (
        "claude-opus-4-1",
        ClaudePrice {
            input: 15.0,
            output: 75.0,
            cache_read: 1.5,
            cache_write: 18.75,
            long_context: None,
        },
    ),
    (
        "claude-opus-4",
        ClaudePrice {
            input: 15.0,
            output: 75.0,
            cache_read: 1.5,
            cache_write: 18.75,
            long_context: None,
        },
    ),
    (
        "claude-sonnet-4-6",
        ClaudePrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
            long_context: None,
        },
    ),
    (
        "claude-sonnet-4-5",
        ClaudePrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
            long_context: Some((200_000, 6.0, 22.5, 0.6, 7.5)),
        },
    ),
    (
        "claude-sonnet-4",
        ClaudePrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
            long_context: Some((200_000, 6.0, 22.5, 0.6, 7.5)),
        },
    ),
    (
        "claude-sonnet-3-7",
        ClaudePrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
            long_context: None,
        },
    ),
    (
        "claude-sonnet-3-5",
        ClaudePrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
            long_context: None,
        },
    ),
    (
        "claude-haiku-4-5",
        ClaudePrice {
            input: 1.0,
            output: 5.0,
            cache_read: 0.1,
            cache_write: 1.25,
            long_context: None,
        },
    ),
    (
        "claude-haiku-3-5",
        ClaudePrice {
            input: 0.8,
            output: 4.0,
            cache_read: 0.08,
            cache_write: 1.0,
            long_context: None,
        },
    ),
    (
        "claude-haiku-3",
        ClaudePrice {
            input: 0.25,
            output: 1.25,
            cache_read: 0.03,
            cache_write: 0.3,
            long_context: None,
        },
    ),
];

// Standard API estimates, not subscription billing. Reviewed 2026-10-23:
// https://developers.openai.com/api/docs/models/gpt-6-astra
// https://developers.openai.com/api/docs/pricing
const CODEX_PRICES: &[(&str, CodexPrice)] = &[
    (
        "gpt-6-astra",
        CodexPrice {
            input: 10.0,
            cached_input: 1.0,
            output: 50.0,
            long_context: Some((272_000, 20.0, 2.0, 75.0)),
        },
    ),
    (
        "gpt-5",
        CodexPrice {
            input: 1.25,
            cached_input: 0.125,
            output: 10.0,
            long_context: None,
        },
    ),
    (
        "gpt-5.1",
        CodexPrice {
            input: 1.25,
            cached_input: 0.125,
            output: 10.0,
            long_context: None,
        },
    ),
    (
        "gpt-5.1-codex",
        CodexPrice {
            input: 1.25,
            cached_input: 0.125,
            output: 10.0,
            long_context: None,
        },
    ),
    (
        "gpt-5.1-codex-max",
        CodexPrice {
            input: 1.25,
            cached_input: 0.125,
            output: 10.0,
            long_context: None,
        },
    ),
    (
        "gpt-5.2",
        CodexPrice {
            input: 1.75,
            cached_input: 0.175,
            output: 14.0,
            long_context: None,
        },
    ),
    (
        "gpt-5.2-codex",
        CodexPrice {
            input: 1.75,
            cached_input: 0.175,
            output: 14.0,
            long_context: None,
        },
    ),
    (
        "gpt-5.3-codex",
        CodexPrice {
            input: 1.75,
            cached_input: 0.175,
            output: 14.0,
            long_context: None,
        },
    ),
    // Spark and the unsuffixed gpt-5.3 have no verified API rate here.
    // Keep them unknown rather than inheriting gpt-5.3-codex prices.
    (
        "gpt-5.4-mini",
        CodexPrice {
            input: 0.75,
            cached_input: 0.075,
            output: 4.5,
            long_context: None,
        },
    ),
    (
        "gpt-5.4-nano",
        CodexPrice {
            input: 0.2,
            cached_input: 0.02,
            output: 1.25,
            long_context: None,
        },
    ),
    (
        "gpt-5.4-pro",
        CodexPrice {
            input: 30.0,
            cached_input: 30.0,
            output: 180.0,
            long_context: Some((272_000, 60.0, 60.0, 270.0)),
        },
    ),
    (
        "gpt-5.4",
        CodexPrice {
            input: 2.5,
            cached_input: 0.25,
            output: 15.0,
            long_context: Some((272_000, 5.0, 0.5, 22.5)),
        },
    ),
    (
        "gpt-5.5-pro",
        CodexPrice {
            input: 30.0,
            cached_input: 30.0,
            output: 180.0,
            long_context: Some((272_000, 60.0, 60.0, 270.0)),
        },
    ),
    (
        "gpt-5.5",
        CodexPrice {
            input: 5.0,
            cached_input: 0.5,
            output: 30.0,
            long_context: Some((272_000, 10.0, 1.0, 45.0)),
        },
    ),
    (
        "gpt-5.6-sol",
        CodexPrice {
            input: 4.0,
            cached_input: 0.4,
            output: 20.0,
            long_context: Some((272_000, 8.0, 0.8, 30.0)),
        },
    ),
    (
        "gpt-5.6-terra",
        CodexPrice {
            input: 2.0,
            cached_input: 0.2,
            output: 12.0,
            long_context: Some((272_000, 4.0, 0.4, 18.0)),
        },
    ),
    (
        "gpt-5.6-luna",
        CodexPrice {
            input: 0.2,
            cached_input: 0.02,
            output: 1.2,
            long_context: Some((272_000, 0.4, 0.04, 1.8)),
        },
    ),
];

fn claude_key(model: &str) -> Option<&'static str> {
    let normalized = model.trim().to_ascii_lowercase().replace('.', "-");
    let normalized = normalized
        .strip_prefix("anthropic/")
        .or_else(|| normalized.strip_prefix("anthropic:"))
        .unwrap_or(&normalized);
    let normalized = normalized.strip_suffix("-thinking").unwrap_or(normalized);
    let normalized = normalized
        .replace("claude-3-7-sonnet", "claude-sonnet-3-7")
        .replace("claude-3-5-sonnet", "claude-sonnet-3-5")
        .replace("claude-3-5-haiku", "claude-haiku-3-5")
        .replace("claude-3-haiku", "claude-haiku-3");
    // Match only known model IDs or dated snapshots. Unknown point releases
    // must never inherit their family's price.
    CLAUDE_PRICES.iter().find_map(|(key, _)| {
        (normalized == *key
            || normalized
                .strip_prefix(&format!("{key}-"))
                .is_some_and(|suffix| {
                    suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit())
                }))
        .then_some(*key)
    })
}

fn strip_reasoning_suffix(mut model: String) -> Option<String> {
    const TIERS: &[&str] = &[
        "minimal", "low", "medium", "high", "xhigh", "max", "ultra", "auto", "none",
    ];
    if let Some(open) = model.rfind('(') {
        if model.ends_with(')') {
            let tier = model[open + 1..model.len() - 1].trim();
            if !TIERS.contains(&tier) {
                return None;
            }
            model.truncate(open);
        }
    }
    for _ in 0..4 {
        let Some(tier) = TIERS
            .iter()
            .find(|tier| model.ends_with(&format!("-{tier}")))
        else {
            break;
        };
        model.truncate(model.len() - tier.len() - 1);
    }
    Some(model.trim().to_string())
}

fn codex_key(model: &str) -> Option<&'static str> {
    let normalized = strip_reasoning_suffix(model.trim().to_ascii_lowercase())?;
    if normalized == "gpt-5" || normalized == "gpt-5-codex" {
        return Some("gpt-5");
    }
    for key in [
        "gpt-6-astra",
        "gpt-5.1-codex-max",
        "gpt-5.1-codex",
        "gpt-5.1",
        "gpt-5.2-codex",
        "gpt-5.2",
        "gpt-5.3-codex",
        "gpt-5.4-mini",
        "gpt-5.4-nano",
        "gpt-5.4-pro",
        "gpt-5.4",
        "gpt-5.5-pro",
        "gpt-5.5",
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-5.6-luna",
    ] {
        if normalized == key
            || normalized
                .strip_prefix(&format!("{key}-"))
                .is_some_and(|suffix| {
                    suffix.len() == 10 && suffix.chars().all(|c| c.is_ascii_digit() || c == '-')
                })
        {
            return Some(key);
        }
    }
    if normalized == "gpt-5.6" {
        return Some("gpt-5.6-sol");
    }
    None
}

#[cfg(test)]
pub fn claude_cost(
    model: Option<&str>,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
) -> Option<f64> {
    claude_request_cost(model, input, output, cache_read, cache_write, 0)
}

pub fn claude_request_cost(
    model: Option<&str>,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cache_write_hour: u64,
) -> Option<f64> {
    let key = claude_key(model?)?;
    let price = CLAUDE_PRICES
        .iter()
        .find(|(candidate, _)| *candidate == key)?
        .1;
    let prompt = input.saturating_add(cache_read).saturating_add(cache_write);
    let (input_rate, output_rate, read_rate, write_rate) = match price.long_context {
        Some((limit, input, output, read, write)) if prompt > limit => (input, output, read, write),
        _ => (
            price.input,
            price.output,
            price.cache_read,
            price.cache_write,
        ),
    };
    let hour = cache_write_hour.min(cache_write);
    Some(
        (input as f64 * input_rate
            + output as f64 * output_rate
            + cache_read as f64 * read_rate
            + (cache_write - hour) as f64 * write_rate
            + hour as f64 * input_rate * 2.0)
            / 1_000_000.0,
    )
}

pub fn codex_cost(model: Option<&str>, input: u64, cached_input: u64, output: u64) -> Option<f64> {
    let key = codex_key(model?)?;
    let price = CODEX_PRICES
        .iter()
        .find(|(candidate, _)| *candidate == key)?
        .1;
    // The entire request uses the long-context rate when its prompt exceeds
    // the threshold, including cached input; output does not set the tier.
    let (input_rate, cached_rate, output_rate) = match price.long_context {
        Some((limit, input_rate, cached, output)) if input > limit => (input_rate, cached, output),
        _ => (price.input, price.cached_input, price.output),
    };
    let cached = cached_input.min(input);
    Some(
        ((input - cached) as f64 * input_rate
            + cached as f64 * cached_rate
            + output as f64 * output_rate)
            / 1_000_000.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_threshold_counts_cached_prompt_and_prices_the_full_request() {
        // 272k is still short context, including all cached input.
        assert!(
            (codex_cost(Some("gpt-6-astra"), 272_000, 272_000, 100).unwrap() - 0.277).abs() < 1e-12
        );
        assert!(
            (codex_cost(Some("gpt-6-astra"), 272_001, 272_000, 100).unwrap() - 0.55152).abs()
                < 1e-12
        );
        // Large output does not trigger the input-context tier.
        assert!((codex_cost(Some("gpt-6-astra"), 100, 0, 300_000).unwrap() - 15.001).abs() < 1e-12);
        // Sonnet 4.5 counts reads and writes towards its 200k prompt tier.
        assert!(
            (claude_request_cost(Some("claude-sonnet-4-5"), 1, 100, 200_000, 1, 1).unwrap()
                - 0.122268)
                .abs()
                < 1e-12
        );
        // Sonnet 4.6 has no long-context premium.
        assert!(
            (claude_request_cost(Some("claude-sonnet-4-6"), 1, 100, 200_000, 1, 1).unwrap()
                - 0.061509)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn unknown_variants_are_not_priced_as_a_known_family() {
        for model in ["gpt-6-astra-pro", "gpt-5.4-unknown", "unmapped"] {
            assert_eq!(codex_cost(Some(model), 10, 0, 10), None);
        }
        assert_eq!(claude_cost(Some("claude-opus-4-99"), 10, 10, 0, 0), None);
        assert!(codex_cost(Some("gpt-6-astra-max"), 10, 0, 10).is_some());
    }

    #[test]
    fn codex_cache_is_not_charged_twice() {
        let cost = codex_cost(Some("gpt-5.6-sol-high"), 1_000_000, 900_000, 100_000).unwrap();
        assert!((cost - 4.52).abs() < 0.001);
    }

    #[test]
    fn current_claude_aliases_are_priced() {
        assert_eq!(
            claude_key("claude-opus-4.8-thinking"),
            Some("claude-opus-4-8")
        );
        assert!(claude_cost(Some("claude-fable-5"), 10, 10, 10, 10).is_some());
    }

    #[test]
    fn every_model_the_picker_offers_has_its_own_price() {
        for (alias, runs) in crate::models::CLAUDE_ALIASES {
            let key = claude_key(runs)
                .unwrap_or_else(|| panic!("`{alias}` runs {runs}, which has no price"));
            // Its own row, not the family's: 5.5 is not billed as 5.
            assert!(runs.starts_with(key), "{runs} is priced as {key}");
            assert_eq!(
                crate::models::claude_label(runs),
                crate::models::claude_label(key)
            );
        }
    }

    #[test]
    fn a_point_release_is_not_priced_as_its_predecessor() {
        assert_eq!(claude_key("claude-opus-5-5"), Some("claude-opus-5-5"));
        assert_eq!(claude_key("claude-opus-5"), Some("claude-opus-5"));
        assert_eq!(claude_key("claude-sonnet-5.5"), Some("claude-sonnet-5-5"));
        assert_eq!(claude_key("claude-fable-5-1"), Some("claude-fable-5-1"));
        assert_eq!(claude_key("claude-fable-5"), Some("claude-fable-5"));
        // One million of each kind of token, at the published rates.
        let cost =
            |model| claude_cost(Some(model), 1_000_000, 1_000_000, 1_000_000, 1_000_000).unwrap();
        assert!((cost("claude-opus-5-5") - 29.2).abs() < 1e-9);
        assert!((cost("claude-sonnet-5-5") - 14.6).abs() < 1e-9);
        assert!((cost("claude-fable-5-1") - 72.75).abs() < 1e-9);
        assert!((cost("claude-haiku-4-5-20251001") - 7.35).abs() < 1e-9);
    }
}
