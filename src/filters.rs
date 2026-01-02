// filters.rs - TOKEN FILTERING LOGIC
#![allow(unused_imports, dead_code)]

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use crate::detection::PumpBuyAccounts;
use crate::socials::{Socials, TokenMetadata};
use crate::config::Config;

/// Check if token passes all filters
/// Returns Ok(None) if passed, Ok(Some(reason)) if failed with specific reason, Err(e) on error
pub fn should_process_token(
    config: &Config,
    accounts: &PumpBuyAccounts,
    socials: Option<&Socials>,
    metadata: Option<&TokenMetadata>,
) -> Result<Option<String>> {
    // Dev buy filter
    let dev_buy_sol = accounts.dev_buy_sol as f64 / 1e9;
    let min_sol = config.min_dev_buy_sol;
    let max_sol = config.max_dev_buy_sol;

    if dev_buy_sol < min_sol || dev_buy_sol > max_sol {
        return Ok(Some(format!("Dev buy SOL ({:.4}) outside range [{:.4}, {:.4}]", dev_buy_sol, min_sol, max_sol)));
    }

    // Social filters (BASIC - require_* filters)
    // These are separate from advanced filters (enable_*)
    // Advanced filters work independently and don't require basic filters to be enabled
    if let Some(socials) = socials {
        if config.require_socials && !socials.has_any() {
            return Ok(Some("Missing required socials".to_string()));
        }

        if config.require_twitter && !socials.has_twitter() {
            return Ok(Some("Missing required Twitter".to_string()));
        }

        if config.require_website && !socials.has_website() {
            return Ok(Some("Missing required website".to_string()));
        }

        if config.require_telegram && !socials.has_telegram() {
            return Ok(Some("Missing required Telegram".to_string()));
        }

        if config.require_discord && !socials.has_discord() {
            return Ok(Some("Missing required Discord".to_string()));
        }

        if config.enable_min_socials_count && socials.count() < config.min_socials_count {
            return Ok(Some(format!("Social count ({}) below minimum ({})", socials.count(), config.min_socials_count)));
        }
    } else {
        // Only reject if BASIC filters (require_*) are enabled and socials are not available
        // Advanced filters (enable_*) will be checked later and handle None socials gracefully
        if config.require_socials {
            return Ok(Some("Missing required socials (socials not available)".to_string()));
        }
        if config.require_twitter {
            return Ok(Some("Missing required Twitter (socials not available)".to_string()));
        }
        if config.require_website {
            return Ok(Some("Missing required website (socials not available)".to_string()));
        }
        if config.require_telegram {
            return Ok(Some("Missing required Telegram (socials not available)".to_string()));
        }
        if config.require_discord {
            return Ok(Some("Missing required Discord (socials not available)".to_string()));
        }
        if config.enable_min_socials_count && config.min_socials_count > 0 {
            return Ok(Some(format!("Social count below minimum ({}) (socials not available)", config.min_socials_count)));
        }
    }

    // Token metadata filters
    if let Some(metadata) = metadata {
        // Uppercase filter: if required, both name and symbol must be uppercase (or empty)
        if config.require_uppercase_token {
            let name_ok = metadata.name.is_empty() || metadata.name == metadata.name.to_uppercase();
            let symbol_ok = metadata.symbol.is_empty() || metadata.symbol == metadata.symbol.to_uppercase();
            if !name_ok || !symbol_ok {
                return Ok(Some("Token name or symbol not uppercase".to_string()));
            }
        }

        // Name length filter: name must be <= max_name_length
        if metadata.name.len() > config.max_name_length {
            return Ok(Some(format!("Name length ({}) exceeds maximum ({})", metadata.name.len(), config.max_name_length)));
        }

        // Ticker length filter: symbol must be between min_ticker_length and max_ticker_length (inclusive)
        let symbol_len = metadata.symbol.len();
        if symbol_len < config.min_ticker_length || symbol_len > config.max_ticker_length {
            return Ok(Some(format!("Symbol length ({}) outside range [{}, {}]", symbol_len, config.min_ticker_length, config.max_ticker_length)));
        }
    } else {
        // If metadata is required for any filter, reject when metadata is not available
        if config.require_uppercase_token || config.max_name_length < usize::MAX || config.min_ticker_length > 0 {
            // Metadata filters are enabled but metadata not available
            // We'll be lenient here - only reject if uppercase is required
            // For length filters, we can't check without metadata, so we skip them
            if config.require_uppercase_token {
                return Ok(Some("Missing required uppercase token (metadata not available)".to_string()));
            }
        }
    }

    // ============================================================================
    // ADVANCED FILTERS - BASIC
    // ============================================================================
    if config.enable_has_twitter && !check_has_twitter(socials) {
        return Ok(Some("Missing Twitter (enable_has_twitter)".to_string()));
    }
    if config.enable_has_telegram && !check_has_telegram(socials) {
        return Ok(Some("Missing Telegram (enable_has_telegram)".to_string()));
    }
    if config.enable_has_website && !check_has_website(socials) {
        return Ok(Some("Missing website (enable_has_website)".to_string()));
    }
    if config.enable_social_count_1_plus && !check_social_count_1_plus(socials) {
        return Ok(Some("Social count less than 1 (enable_social_count_1_plus)".to_string()));
    }
    if config.enable_social_count_2_plus && !check_social_count_2_plus(socials) {
        return Ok(Some("Social count less than 2 (enable_social_count_2_plus)".to_string()));
    }
    if config.enable_social_count_3 && !check_social_count_3(socials) {
        return Ok(Some("Social count not equal to 3 (enable_social_count_3)".to_string()));
    }
    if config.enable_website_com && !check_website_com(socials) {
        return Ok(Some("Website is not .com (enable_website_com)".to_string()));
    }
    if config.enable_website_org && !check_website_org(socials) {
        return Ok(Some("Website is not .org (enable_website_org)".to_string()));
    }
    if config.enable_website_xyz && !check_website_xyz(socials) {
        return Ok(Some("Website is not .xyz (enable_website_xyz)".to_string()));
    }
    if config.enable_uppercase && !check_uppercase_symbol(metadata) {
        return Ok(Some("Symbol is not uppercase (enable_uppercase)".to_string()));
    }
    if config.enable_lowercase && !check_lowercase_symbol(metadata) {
        return Ok(Some("Symbol is not lowercase (enable_lowercase)".to_string()));
    }
    if config.enable_symbol_3_4 && !check_symbol_3_4(metadata) {
        return Ok(Some("Symbol length not 3-4 (enable_symbol_3_4)".to_string()));
    }
    if config.enable_symbol_3_6 && !check_symbol_3_6(metadata) {
        return Ok(Some("Symbol length not 3-6 (enable_symbol_3_6)".to_string()));
    }
    if config.enable_symbol_3_7 && !check_symbol_3_7(metadata) {
        return Ok(Some("Symbol length not 3-7 (enable_symbol_3_7)".to_string()));
    }
    if config.enable_name_short && !check_name_short(metadata) {
        return Ok(Some("Name length greater than 20 (enable_name_short)".to_string()));
    }
    if config.enable_name_medium && !check_name_medium(metadata) {
        return Ok(Some("Name length not 11-20 (enable_name_medium)".to_string()));
    }

    // ============================================================================
    // ADVANCED FILTERS - TWITTER TYPES
    // ============================================================================
    if config.enable_twitter_account && !check_twitter_account(socials) {
        return Ok(Some("Twitter is not account type (enable_twitter_account)".to_string()));
    }
    if config.enable_twitter_community && !check_twitter_community(socials) {
        return Ok(Some("Twitter is not community type (enable_twitter_community)".to_string()));
    }
    if config.enable_twitter_status && !check_twitter_status(socials) {
        return Ok(Some("Twitter is not status type (enable_twitter_status)".to_string()));
    }
    if config.enable_twitter_no_status && !check_twitter_no_status(socials) {
        return Ok(Some("Twitter is status type (enable_twitter_no_status)".to_string()));
    }
    if config.enable_twitter_account_or_community && !check_twitter_account_or_community(socials) {
        return Ok(Some("Twitter is not account or community type (enable_twitter_account_or_community)".to_string()));
    }
    if config.enable_twitter_username_length_short && !check_twitter_username_length_short(socials) {
        return Ok(Some("Twitter username length greater than 15 (enable_twitter_username_length_short)".to_string()));
    }
    if config.enable_twitter_username_length_medium && !check_twitter_username_length_medium(socials) {
        return Ok(Some("Twitter username length not 15-25 (enable_twitter_username_length_medium)".to_string()));
    }
    if config.enable_has_twitter_with_username && !check_has_twitter_with_username(socials) {
        return Ok(Some("Missing Twitter with username (enable_has_twitter_with_username)".to_string()));
    }

    // ============================================================================
    // ADVANCED FILTERS - BRAND MATCHING
    // ============================================================================
    if config.enable_has_brand_match && !check_has_brand_match(metadata, socials) {
        return Ok(Some("No brand match (enable_has_brand_match)".to_string()));
    }
    if config.enable_brand_score_2_plus && !check_brand_score_2_plus(metadata, socials) {
        return Ok(Some("Brand score less than 2 (enable_brand_score_2_plus)".to_string()));
    }
    if config.enable_brand_score_3_plus && !check_brand_score_3_plus(metadata, socials) {
        return Ok(Some("Brand score less than 3 (enable_brand_score_3_plus)".to_string()));
    }
    if config.enable_brand_score_4_plus && !check_brand_score_4_plus(metadata, socials) {
        return Ok(Some("Brand score less than 4 (enable_brand_score_4_plus)".to_string()));
    }
    if config.enable_perfect_brand_match && !check_perfect_brand_match(metadata, socials) {
        return Ok(Some("Not perfect brand match (enable_perfect_brand_match)".to_string()));
    }
    if config.enable_twitter_matches_website && !check_twitter_matches_website(socials) {
        return Ok(Some("Twitter does not match website (enable_twitter_matches_website)".to_string()));
    }
    if config.enable_name_matches_website && !check_name_matches_website(metadata, socials) {
        return Ok(Some("Name does not match website (enable_name_matches_website)".to_string()));
    }
    if config.enable_symbol_matches_website && !check_symbol_matches_website(metadata, socials) {
        return Ok(Some("Symbol does not match website (enable_symbol_matches_website)".to_string()));
    }
    if config.enable_name_matches_twitter && !check_name_matches_twitter(metadata, socials) {
        return Ok(Some("Name does not match Twitter (enable_name_matches_twitter)".to_string()));
    }
    if config.enable_symbol_matches_twitter && !check_symbol_matches_twitter(metadata, socials) {
        return Ok(Some("Symbol does not match Twitter (enable_symbol_matches_twitter)".to_string()));
    }

    // ============================================================================
    // ADVANCED FILTERS - COMBINED BRAND MATCHING
    // ============================================================================
    if config.enable_name_matches_both && !check_name_matches_both(metadata, socials) {
        return Ok(Some("Name does not match both website and Twitter (enable_name_matches_both)".to_string()));
    }
    if config.enable_symbol_matches_both && !check_symbol_matches_both(metadata, socials) {
        return Ok(Some("Symbol does not match both website and Twitter (enable_symbol_matches_both)".to_string()));
    }
    if config.enable_twitter_and_name_match_website && !check_twitter_and_name_match_website(metadata, socials) {
        return Ok(Some("Twitter and name do not match website (enable_twitter_and_name_match_website)".to_string()));
    }
    if config.enable_twitter_and_symbol_match_website && !check_twitter_and_symbol_match_website(metadata, socials) {
        return Ok(Some("Twitter and symbol do not match website (enable_twitter_and_symbol_match_website)".to_string()));
    }
    if config.enable_name_and_symbol_match_twitter && !check_name_and_symbol_match_twitter(metadata, socials) {
        return Ok(Some("Name and symbol do not match Twitter (enable_name_and_symbol_match_twitter)".to_string()));
    }
    if config.enable_brand_match_and_twitter && !check_brand_match_and_twitter(metadata, socials) {
        return Ok(Some("No brand match or missing Twitter (enable_brand_match_and_twitter)".to_string()));
    }
    if config.enable_brand_match_and_website && !check_brand_match_and_website(metadata, socials) {
        return Ok(Some("No brand match or missing website (enable_brand_match_and_website)".to_string()));
    }
    if config.enable_brand_match_and_twitter_and_website && !check_brand_match_and_twitter_and_website(metadata, socials) {
        return Ok(Some("No brand match or missing Twitter or website (enable_brand_match_and_twitter_and_website)".to_string()));
    }
    if config.enable_perfect_brand_and_twitter && !check_perfect_brand_and_twitter(metadata, socials) {
        return Ok(Some("Not perfect brand match or missing Twitter (enable_perfect_brand_and_twitter)".to_string()));
    }
    if config.enable_perfect_brand_and_website && !check_perfect_brand_and_website(metadata, socials) {
        return Ok(Some("Not perfect brand match or missing website (enable_perfect_brand_and_website)".to_string()));
    }
    if config.enable_perfect_brand_and_twitter_community && !check_perfect_brand_and_twitter_community(metadata, socials) {
        return Ok(Some("Not perfect brand match or Twitter is not community (enable_perfect_brand_and_twitter_community)".to_string()));
    }
    if config.enable_brand_score_3_plus_and_com && !check_brand_score_3_plus_and_com(metadata, socials) {
        return Ok(Some("Brand score less than 3 or website is not .com (enable_brand_score_3_plus_and_com)".to_string()));
    }
    if config.enable_brand_score_4_plus_and_com && !check_brand_score_4_plus_and_com(metadata, socials) {
        return Ok(Some("Brand score less than 4 or website is not .com (enable_brand_score_4_plus_and_com)".to_string()));
    }
    if config.enable_twitter_match_website_and_com && !check_twitter_match_website_and_com(socials) {
        return Ok(Some("Twitter does not match website or website is not .com (enable_twitter_match_website_and_com)".to_string()));
    }
    if config.enable_name_match_website_and_com && !check_name_match_website_and_com(metadata, socials) {
        return Ok(Some("Name does not match website or website is not .com (enable_name_match_website_and_com)".to_string()));
    }

    Ok(None)
}

/// Check creator token count filter
pub fn check_creator_token_count(
    count: u32,
    config: &Config,
) -> bool {
    count >= config.min_dev_tokens as u32 && count <= config.max_dev_tokens as u32
}

// ============================================================================
// HELPER FUNCTIONS FOR PARSING
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwitterType {
    Account,
    Community,
    Status,
    Unknown,
}

/// Extract Twitter type from URL
pub fn get_twitter_type(url: &str) -> TwitterType {
    let url_lower = url.to_lowercase();
    if url_lower.contains("/i/communities/") {
        TwitterType::Community
    } else if url_lower.contains("/status/") {
        TwitterType::Status
    } else if url_lower.contains("x.com/") || url_lower.contains("twitter.com/") {
        // Check if it's a valid account URL (not community or status)
        let parts: Vec<&str> = url_lower.split('/').collect();
        if parts.len() >= 2 {
            let last_part = parts[parts.len() - 1];
            // If it's just a username (no status, no communities), it's an account
            if !last_part.contains("status") && !last_part.contains("communities") {
                return TwitterType::Account;
            }
        }
        TwitterType::Account
    } else {
        TwitterType::Unknown
    }
}

/// Extract Twitter username from URL
pub fn extract_twitter_username(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();
    // Handle x.com and twitter.com URLs
    let patterns = ["x.com/", "twitter.com/"];
    for pattern in &patterns {
        if let Some(pos) = url_lower.find(pattern) {
            let start = pos + pattern.len();
            let remaining = &url_lower[start..];
            // Extract username (stop at / or ? or end)
            let username_end = remaining
                .find('/')
                .or_else(|| remaining.find('?'))
                .unwrap_or(remaining.len());
            let username = &remaining[..username_end];
            if !username.is_empty() && !username.contains("i/") {
                return Some(username.to_string());
            }
        }
    }
    None
}

/// Extract website domain TLD
pub fn extract_website_domain(url: &str) -> Option<&'static str> {
    let url_lower = url.to_lowercase();
    if url_lower.ends_with(".com") || url_lower.contains(".com/") || url_lower.contains(".com?") {
        Some(".com")
    } else if url_lower.ends_with(".org") || url_lower.contains(".org/") || url_lower.contains(".org?") {
        Some(".org")
    } else if url_lower.ends_with(".xyz") || url_lower.contains(".xyz/") || url_lower.contains(".xyz?") {
        Some(".xyz")
    } else {
        None
    }
}

/// Normalize string for comparison (lowercase, remove spaces, special chars)
fn normalize_for_match(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Check if two strings match (case-insensitive, alphanumeric only)
fn strings_match(s1: &str, s2: &str) -> bool {
    normalize_for_match(s1) == normalize_for_match(s2)
}

// ============================================================================
// BRAND MATCHING LOGIC
// ============================================================================

/// Calculate brand score (0-4)
/// Score 1: name matches website
/// Score 2: symbol matches website
/// Score 3: name matches twitter
/// Score 4: symbol matches twitter
pub fn calculate_brand_score(
    name: &str,
    symbol: &str,
    website: Option<&str>,
    twitter: Option<&str>,
) -> u8 {
    let mut score = 0;
    
    if let Some(web) = website {
        if strings_match(name, web) {
            score += 1;
        }
        if strings_match(symbol, web) {
            score += 1;
        }
    }
    
    if let Some(tw) = twitter {
        // Extract username from Twitter URL for comparison
        if let Some(username) = extract_twitter_username(tw) {
            if strings_match(name, &username) {
                score += 1;
            }
            if strings_match(symbol, &username) {
                score += 1;
            }
        }
    }
    
    score
}

/// Check if there's any brand match (score >= 1)
pub fn has_brand_match(
    name: &str,
    symbol: &str,
    website: Option<&str>,
    twitter: Option<&str>,
) -> bool {
    calculate_brand_score(name, symbol, website, twitter) >= 1
}

/// Check if perfect brand match (score == 4)
pub fn is_perfect_brand_match(
    name: &str,
    symbol: &str,
    website: Option<&str>,
    twitter: Option<&str>,
) -> bool {
    calculate_brand_score(name, symbol, website, twitter) == 4
}

/// Check if name matches website
pub fn name_matches_website(name: &str, website: Option<&str>) -> bool {
    website.map(|w| strings_match(name, w)).unwrap_or(false)
}

/// Check if symbol matches website
pub fn symbol_matches_website(symbol: &str, website: Option<&str>) -> bool {
    website.map(|w| strings_match(symbol, w)).unwrap_or(false)
}

/// Check if name matches Twitter username
pub fn name_matches_twitter(name: &str, twitter: Option<&str>) -> bool {
    twitter
        .and_then(|t| extract_twitter_username(t))
        .map(|u| strings_match(name, &u))
        .unwrap_or(false)
}

/// Check if symbol matches Twitter username
pub fn symbol_matches_twitter(symbol: &str, twitter: Option<&str>) -> bool {
    twitter
        .and_then(|t| extract_twitter_username(t))
        .map(|u| strings_match(symbol, &u))
        .unwrap_or(false)
}

/// Check if Twitter username matches website
pub fn twitter_matches_website(twitter: Option<&str>, website: Option<&str>) -> bool {
    if let (Some(tw), Some(web)) = (twitter, website) {
        if let Some(username) = extract_twitter_username(tw) {
            return strings_match(&username, web);
        }
    }
    false
}

// ============================================================================
// BASIC FILTER FUNCTIONS
// ============================================================================

/// Check if has Twitter
pub fn check_has_twitter(socials: Option<&Socials>) -> bool {
    socials.map(|s| s.has_twitter()).unwrap_or(false)
}

/// Check if has Telegram
pub fn check_has_telegram(socials: Option<&Socials>) -> bool {
    socials.map(|s| s.has_telegram()).unwrap_or(false)
}

/// Check if has Website
pub fn check_has_website(socials: Option<&Socials>) -> bool {
    socials.map(|s| s.has_website()).unwrap_or(false)
}

/// Check if social count >= 1
pub fn check_social_count_1_plus(socials: Option<&Socials>) -> bool {
    socials.map(|s| s.count() >= 1).unwrap_or(false)
}

/// Check if social count >= 2
pub fn check_social_count_2_plus(socials: Option<&Socials>) -> bool {
    socials.map(|s| s.count() >= 2).unwrap_or(false)
}

/// Check if social count == 3
pub fn check_social_count_3(socials: Option<&Socials>) -> bool {
    socials.map(|s| s.count() == 3).unwrap_or(false)
}

/// Check if website is .com
pub fn check_website_com(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.website.as_deref())
        .and_then(|url| extract_website_domain(url))
        .map(|domain| domain == ".com")
        .unwrap_or(false)
}

/// Check if website is .org
pub fn check_website_org(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.website.as_deref())
        .and_then(|url| extract_website_domain(url))
        .map(|domain| domain == ".org")
        .unwrap_or(false)
}

/// Check if website is .xyz
pub fn check_website_xyz(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.website.as_deref())
        .and_then(|url| extract_website_domain(url))
        .map(|domain| domain == ".xyz")
        .unwrap_or(false)
}

/// Check if symbol is uppercase
pub fn check_uppercase_symbol(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| !m.symbol.is_empty() && m.symbol == m.symbol.to_uppercase())
        .unwrap_or(false)
}

/// Check if symbol is lowercase
pub fn check_lowercase_symbol(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| !m.symbol.is_empty() && m.symbol == m.symbol.to_lowercase())
        .unwrap_or(false)
}

/// Check if symbol length is 3-4
pub fn check_symbol_3_4(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| (3..=4).contains(&m.symbol.len()))
        .unwrap_or(false)
}

/// Check if symbol length is 3-6
pub fn check_symbol_3_6(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| (3..=6).contains(&m.symbol.len()))
        .unwrap_or(false)
}

/// Check if symbol length is 3-7
pub fn check_symbol_3_7(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| (3..=7).contains(&m.symbol.len()))
        .unwrap_or(false)
}

/// Check if name length <= 20
pub fn check_name_short(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| m.name.len() <= 20)
        .unwrap_or(false)
}

/// Check if name length is 11-20
pub fn check_name_medium(metadata: Option<&TokenMetadata>) -> bool {
    metadata
        .map(|m| (11..=20).contains(&m.name.len()))
        .unwrap_or(false)
}

// ============================================================================
// TWITTER TYPE FILTER FUNCTIONS
// ============================================================================

/// Check if Twitter is account type
pub fn check_twitter_account(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .map(|url| get_twitter_type(url) == TwitterType::Account)
        .unwrap_or(false)
}

/// Check if Twitter is community type
pub fn check_twitter_community(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .map(|url| get_twitter_type(url) == TwitterType::Community)
        .unwrap_or(false)
}

/// Check if Twitter is status type
pub fn check_twitter_status(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .map(|url| get_twitter_type(url) == TwitterType::Status)
        .unwrap_or(false)
}

/// Check if Twitter is NOT status type
pub fn check_twitter_no_status(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .map(|url| get_twitter_type(url) != TwitterType::Status)
        .unwrap_or(false)
}

/// Check if Twitter is account OR community
pub fn check_twitter_account_or_community(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .map(|url| {
            let ttype = get_twitter_type(url);
            ttype == TwitterType::Account || ttype == TwitterType::Community
        })
        .unwrap_or(false)
}

/// Check if Twitter username length <= 15
pub fn check_twitter_username_length_short(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .and_then(|url| extract_twitter_username(url))
        .map(|username| username.len() <= 15)
        .unwrap_or(false)
}

/// Check if Twitter username length is 15-25
pub fn check_twitter_username_length_medium(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .and_then(|url| extract_twitter_username(url))
        .map(|username| (15..=25).contains(&username.len()))
        .unwrap_or(false)
}

/// Check if has Twitter with username (not community/status)
pub fn check_has_twitter_with_username(socials: Option<&Socials>) -> bool {
    socials
        .and_then(|s| s.twitter.as_deref())
        .and_then(|url| {
            let ttype = get_twitter_type(url);
            if ttype == TwitterType::Account {
                extract_twitter_username(url)
            } else {
                None
            }
        })
        .is_some()
}

// ============================================================================
// BRAND MATCHING FILTER FUNCTIONS
// ============================================================================

/// Check if has brand match (score >= 1)
pub fn check_has_brand_match(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        has_brand_match(
            &meta.name,
            &meta.symbol,
            soc.website.as_deref(),
            soc.twitter.as_deref(),
        )
    } else {
        false
    }
}

/// Check if brand score >= 2
pub fn check_brand_score_2_plus(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        calculate_brand_score(
            &meta.name,
            &meta.symbol,
            soc.website.as_deref(),
            soc.twitter.as_deref(),
        ) >= 2
    } else {
        false
    }
}

/// Check if brand score >= 3
pub fn check_brand_score_3_plus(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        calculate_brand_score(
            &meta.name,
            &meta.symbol,
            soc.website.as_deref(),
            soc.twitter.as_deref(),
        ) >= 3
    } else {
        false
    }
}

/// Check if brand score >= 4
pub fn check_brand_score_4_plus(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        calculate_brand_score(
            &meta.name,
            &meta.symbol,
            soc.website.as_deref(),
            soc.twitter.as_deref(),
        ) >= 4
    } else {
        false
    }
}

/// Check if perfect brand match
pub fn check_perfect_brand_match(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        is_perfect_brand_match(
            &meta.name,
            &meta.symbol,
            soc.website.as_deref(),
            soc.twitter.as_deref(),
        )
    } else {
        false
    }
}

/// Check if Twitter matches website
pub fn check_twitter_matches_website(socials: Option<&Socials>) -> bool {
    if let Some(soc) = socials {
        twitter_matches_website(soc.twitter.as_deref(), soc.website.as_deref())
    } else {
        false
    }
}

/// Check if name matches website
pub fn check_name_matches_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        name_matches_website(&meta.name, soc.website.as_deref())
    } else {
        false
    }
}

/// Check if symbol matches website
pub fn check_symbol_matches_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        symbol_matches_website(&meta.symbol, soc.website.as_deref())
    } else {
        false
    }
}

/// Check if name matches Twitter
pub fn check_name_matches_twitter(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        name_matches_twitter(&meta.name, soc.twitter.as_deref())
    } else {
        false
    }
}

/// Check if symbol matches Twitter
pub fn check_symbol_matches_twitter(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        symbol_matches_twitter(&meta.symbol, soc.twitter.as_deref())
    } else {
        false
    }
}

// ============================================================================
// COMBINED BRAND MATCHING FILTER FUNCTIONS
// ============================================================================

/// Check if name matches both website AND Twitter
pub fn check_name_matches_both(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        name_matches_website(&meta.name, soc.website.as_deref())
            && name_matches_twitter(&meta.name, soc.twitter.as_deref())
    } else {
        false
    }
}

/// Check if symbol matches both website AND Twitter
pub fn check_symbol_matches_both(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        symbol_matches_website(&meta.symbol, soc.website.as_deref())
            && symbol_matches_twitter(&meta.symbol, soc.twitter.as_deref())
    } else {
        false
    }
}

/// Check if Twitter AND name match website
pub fn check_twitter_and_name_match_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        twitter_matches_website(soc.twitter.as_deref(), soc.website.as_deref())
            && name_matches_website(&meta.name, soc.website.as_deref())
    } else {
        false
    }
}

/// Check if Twitter AND symbol match website
pub fn check_twitter_and_symbol_match_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        twitter_matches_website(soc.twitter.as_deref(), soc.website.as_deref())
            && symbol_matches_website(&meta.symbol, soc.website.as_deref())
    } else {
        false
    }
}

/// Check if name AND symbol match Twitter
pub fn check_name_and_symbol_match_twitter(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    if let (Some(meta), Some(soc)) = (metadata, socials) {
        name_matches_twitter(&meta.name, soc.twitter.as_deref())
            && symbol_matches_twitter(&meta.symbol, soc.twitter.as_deref())
    } else {
        false
    }
}

/// Check if brand match AND has Twitter
pub fn check_brand_match_and_twitter(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_has_brand_match(metadata, socials) && check_has_twitter(socials)
}

/// Check if brand match AND has website
pub fn check_brand_match_and_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_has_brand_match(metadata, socials) && check_has_website(socials)
}

/// Check if brand match AND has Twitter AND has website
pub fn check_brand_match_and_twitter_and_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_has_brand_match(metadata, socials)
        && check_has_twitter(socials)
        && check_has_website(socials)
}

/// Check if perfect brand match AND has Twitter
pub fn check_perfect_brand_and_twitter(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_perfect_brand_match(metadata, socials) && check_has_twitter(socials)
}

/// Check if perfect brand match AND has website
pub fn check_perfect_brand_and_website(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_perfect_brand_match(metadata, socials) && check_has_website(socials)
}

/// Check if perfect brand match AND Twitter is community
pub fn check_perfect_brand_and_twitter_community(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_perfect_brand_match(metadata, socials) && check_twitter_community(socials)
}

/// Check if brand score >= 3 AND website is .com
pub fn check_brand_score_3_plus_and_com(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_brand_score_3_plus(metadata, socials) && check_website_com(socials)
}

/// Check if brand score >= 4 AND website is .com
pub fn check_brand_score_4_plus_and_com(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_brand_score_4_plus(metadata, socials) && check_website_com(socials)
}

/// Check if Twitter matches website AND website is .com
pub fn check_twitter_match_website_and_com(socials: Option<&Socials>) -> bool {
    check_twitter_matches_website(socials) && check_website_com(socials)
}

/// Check if name matches website AND website is .com
pub fn check_name_match_website_and_com(
    metadata: Option<&TokenMetadata>,
    socials: Option<&Socials>,
) -> bool {
    check_name_matches_website(metadata, socials) && check_website_com(socials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socials::Socials;

    fn create_test_config() -> Config {
        Config::default()
    }

    fn create_test_accounts(dev_buy_sol: f64) -> PumpBuyAccounts {
        PumpBuyAccounts {
            mint: Pubkey::new_unique(),
            bonding_curve: Pubkey::new_unique(),
            associated_bonding_curve: Pubkey::new_unique(),
            creator_vault: Pubkey::new_unique(),
            event_authority: Pubkey::new_unique(),
            global_volume: Pubkey::new_unique(),
            global: Pubkey::new_unique(),
            fee_recipient: Pubkey::new_unique(),
            fee_config: Pubkey::new_unique(),
            fee_program: Pubkey::new_unique(),
            dev_buy_sol: (dev_buy_sol * 1e9) as u64,
            creator: Pubkey::new_unique(),
            associated_bonding_curve_instruction: None,
        }
    }

    #[test]
    fn test_should_process_token_dev_buy_filter() {
        let config = create_test_config();
        
        // Valid dev buy: 0.8 SOL = 0.8 * 162 = 129.6 USD (outside 500-1200 range)
        // Need to use value in range: 500/162 = 3.09 SOL to 1200/162 = 7.41 SOL
        let accounts = create_test_accounts(5.0); // 5 SOL = 810 USD (within range)
        assert!(should_process_token(&config, &accounts, None, None).unwrap().is_none());

        // Too low
        let accounts_low = create_test_accounts(0.001);
        assert!(should_process_token(&config, &accounts_low, None, None).unwrap().is_some());

        // Too high
        let accounts_high = create_test_accounts(10.0);
        assert!(should_process_token(&config, &accounts_high, None, None).unwrap().is_some());
    }

    #[test]
    fn test_should_process_token_social_filters() {
        let mut config = create_test_config();
        config.require_socials = true;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // No socials - should fail
        assert!(should_process_token(&config, &accounts, None, None).unwrap().is_some());

        // With socials - should pass
        let socials = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials), None).unwrap().is_none());
    }

    #[test]
    fn test_should_process_token_twitter_filter() {
        let mut config = create_test_config();
        config.require_twitter = true;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // No twitter - should fail
        let socials_no_twitter = Socials {
            twitter: None,
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_no_twitter), None).unwrap().is_some());

        // With twitter - should pass
        let socials_with_twitter = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_with_twitter), None).unwrap().is_none());
    }

    #[test]
    fn test_should_process_token_website_filter() {
        let mut config = create_test_config();
        config.require_website = true;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // No website - should fail
        let socials_no_website = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_no_website), None).unwrap().is_some());

        // With website - should pass
        let socials_with_website = Socials {
            twitter: None,
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_with_website), None).unwrap().is_none());

        // With both twitter and website - should pass
        let socials_both = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_both), None).unwrap().is_none());
    }

    #[test]
    fn test_should_process_token_min_socials_count() {
        let mut config = create_test_config();
        config.enable_min_socials_count = true;
        config.min_socials_count = 2;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // Only 1 social - should fail
        let socials_one = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_one), None).unwrap().is_some());

        // 2+ socials - should pass
        let socials_two = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_two), None).unwrap().is_none());
    }

    #[test]
    fn test_check_creator_token_count() {
        let mut config = create_test_config();
        config.min_dev_tokens = 6;
        config.max_dev_tokens = 10;
        
        // Valid count (within range)
        assert!(check_creator_token_count(7, &config), "7 should pass (within 6-10)");
        assert!(check_creator_token_count(6, &config), "6 should pass (min boundary)");
        assert!(check_creator_token_count(10, &config), "10 should pass (max boundary)");

        // Too low
        assert!(!check_creator_token_count(5, &config), "5 should fail (below min 6)");
        assert!(!check_creator_token_count(0, &config), "0 should fail (below min 6)");

        // Too high
        assert!(!check_creator_token_count(11, &config), "11 should fail (above max 10)");
        assert!(!check_creator_token_count(520, &config), "520 should fail (above max 10)");
        assert!(!check_creator_token_count(8500, &config), "8500 should fail (above max 10)");
    }

    #[test]
    fn test_check_creator_token_count_edge_cases() {
        let mut config = create_test_config();
        
        // Test with filter 0-10 (default)
        config.min_dev_tokens = 0;
        config.max_dev_tokens = 10;
        
        assert!(check_creator_token_count(0, &config), "0 should pass (min is 0)");
        assert!(check_creator_token_count(5, &config), "5 should pass");
        assert!(check_creator_token_count(10, &config), "10 should pass (max boundary)");
        assert!(!check_creator_token_count(11, &config), "11 should fail");
        assert!(!check_creator_token_count(520, &config), "520 should fail");
        
        // Test with filter 1-5
        config.min_dev_tokens = 1;
        config.max_dev_tokens = 5;
        
        assert!(!check_creator_token_count(0, &config), "0 should fail (below min 1)");
        assert!(check_creator_token_count(1, &config), "1 should pass");
        assert!(check_creator_token_count(3, &config), "3 should pass");
        assert!(check_creator_token_count(5, &config), "5 should pass");
        assert!(!check_creator_token_count(6, &config), "6 should fail (above max 5)");
    }

    #[test]
    fn test_filter_logic_simulation() {
        // Simulate the filter logic from bot_core.rs
        let mut config = create_test_config();
        config.min_dev_tokens = 0;
        config.max_dev_tokens = 10;
        
        // Simulate DAS API returning different counts
        let test_cases = vec![
            (0u32, true, "0 tokens should pass (min is 0)"),
            (5u32, true, "5 tokens should pass"),
            (10u32, true, "10 tokens should pass (max boundary)"),
            (11u32, false, "11 tokens should fail (above max)"),
            (520u32, false, "520 tokens should fail (way above max)"),
            (8500u32, false, "8500 tokens should fail (way above max)"),
        ];
        
        for (count, should_pass, msg) in test_cases {
            let passes = count >= config.min_dev_tokens as u32 && count <= config.max_dev_tokens as u32;
            assert_eq!(passes, should_pass, "{}: count={}, expected={}", msg, count, should_pass);
        }
    }

    #[test]
    fn test_uppercase_filter() {
        use crate::socials::TokenMetadata;
        
        let mut config = create_test_config();
        config.require_uppercase_token = true;
        
        let accounts = create_test_accounts(5.0);
        
        // Uppercase name and symbol - should pass
        let metadata_upper = TokenMetadata {
            name: "TOKEN NAME".to_string(),
            symbol: "TKN".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_upper)).unwrap().is_none());
        
        // Lowercase name - should fail
        let metadata_lower_name = TokenMetadata {
            name: "token name".to_string(),
            symbol: "TKN".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_lower_name)).unwrap().is_some());
        
        // Lowercase symbol - should fail
        let metadata_lower_symbol = TokenMetadata {
            name: "TOKEN NAME".to_string(),
            symbol: "tkn".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_lower_symbol)).unwrap().is_some());
        
        // Empty name and symbol - should pass (empty is allowed)
        // Need to set min_ticker_length to 0 to allow empty symbols
        config.min_ticker_length = 0;
        let metadata_empty = TokenMetadata {
            name: String::new(),
            symbol: String::new(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_empty)).unwrap().is_none());
        
        // Filter disabled - should pass regardless
        config.require_uppercase_token = false;
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_lower_name)).unwrap().is_none());
    }

    #[test]
    fn test_name_length_filter() {
        use crate::socials::TokenMetadata;
        
        let mut config = create_test_config();
        config.max_name_length = 20;
        
        let accounts = create_test_accounts(5.0);
        
        // Name exactly at max - should pass
        let metadata_exact = TokenMetadata {
            name: "A".repeat(20),
            symbol: "TKN".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_exact)).unwrap().is_none());
        
        // Name below max - should pass
        let metadata_short = TokenMetadata {
            name: "Short Name".to_string(),
            symbol: "TKN".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_short)).unwrap().is_none());
        
        // Name above max - should fail
        let metadata_long = TokenMetadata {
            name: "A".repeat(21),
            symbol: "TKN".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_long)).unwrap().is_some());
        
        // Empty name - should pass
        let metadata_empty = TokenMetadata {
            name: String::new(),
            symbol: "TKN".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_empty)).unwrap().is_none());
    }

    #[test]
    fn test_ticker_length_filter() {
        use crate::socials::TokenMetadata;
        
        let mut config = create_test_config();
        config.min_ticker_length = 3;
        config.max_ticker_length = 7;
        
        let accounts = create_test_accounts(5.0);
        
        // Ticker at min length - should pass
        let metadata_min = TokenMetadata {
            name: "Token Name".to_string(),
            symbol: "ABC".to_string(), // 3 chars
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_min)).unwrap().is_none());
        
        // Ticker at max length - should pass
        let metadata_max = TokenMetadata {
            name: "Token Name".to_string(),
            symbol: "ABCDEFG".to_string(), // 7 chars
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_max)).unwrap().is_none());
        
        // Ticker in range - should pass
        let metadata_mid = TokenMetadata {
            name: "Token Name".to_string(),
            symbol: "TOKEN".to_string(), // 5 chars
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_mid)).unwrap().is_none());
        
        // Ticker below min - should fail
        let metadata_short = TokenMetadata {
            name: "Token Name".to_string(),
            symbol: "AB".to_string(), // 2 chars
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_short)).unwrap().is_some());
        
        // Ticker above max - should fail
        let metadata_long = TokenMetadata {
            name: "Token Name".to_string(),
            symbol: "ABCDEFGH".to_string(), // 8 chars
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_long)).unwrap().is_some());
        
        // Empty ticker - should fail (below min)
        let metadata_empty = TokenMetadata {
            name: "Token Name".to_string(),
            symbol: String::new(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_empty)).unwrap().is_some());
    }

    #[test]
    fn test_combined_metadata_filters() {
        use crate::socials::TokenMetadata;
        
        let mut config = create_test_config();
        config.require_uppercase_token = true;
        config.max_name_length = 20;
        config.min_ticker_length = 3;
        config.max_ticker_length = 7;
        
        let accounts = create_test_accounts(5.0);
        
        // All filters pass
        let metadata_valid = TokenMetadata {
            name: "VALID TOKEN NAME".to_string(), // 17 chars, uppercase
            symbol: "VALID".to_string(), // 5 chars, uppercase
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_valid)).unwrap().is_none());
        
        // Fails uppercase check
        let metadata_lower = TokenMetadata {
            name: "valid token name".to_string(),
            symbol: "VALID".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_lower)).unwrap().is_some());
        
        // Fails name length
        let metadata_long_name = TokenMetadata {
            name: "A".repeat(21),
            symbol: "VALID".to_string(),
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_long_name)).unwrap().is_some());
        
        // Fails ticker length
        let metadata_short_ticker = TokenMetadata {
            name: "VALID TOKEN NAME".to_string(),
            symbol: "VA".to_string(), // Too short
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_short_ticker)).unwrap().is_some());
    }

    #[test]
    fn test_ticker_3_7_and_twitter_community_filter() {
        let mut config = create_test_config();
        // Set ticker length filter: 3-7 characters
        config.min_ticker_length = 3;
        config.max_ticker_length = 7;
        // Enable Twitter Community filter
        config.enable_twitter_community = true;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount

        // Test 1: Valid token - ticker length 4 (in range 3-7) + Twitter Community
        let metadata_valid = TokenMetadata {
            name: "Test Token".to_string(),
            symbol: "TEST".to_string(), // 4 chars, in range 3-7
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        let socials_community = Socials {
            twitter: Some("https://x.com/i/communities/123456".to_string()), // Community URL
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_community), Some(&metadata_valid)).unwrap().is_none(),
            "Token with ticker length 4 and Twitter Community should pass");

        // Test 2: Invalid - ticker too short (2 chars)
        let metadata_short = TokenMetadata {
            name: "Test Token".to_string(),
            symbol: "TE".to_string(), // 2 chars, too short
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_community), Some(&metadata_short)).unwrap().is_some(),
            "Token with ticker length 2 should fail");

        // Test 3: Invalid - ticker too long (8 chars)
        let metadata_long = TokenMetadata {
            name: "Test Token".to_string(),
            symbol: "TESTTEST".to_string(), // 8 chars, too long
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_community), Some(&metadata_long)).unwrap().is_some(),
            "Token with ticker length 8 should fail");

        // Test 4: Invalid - Twitter Account (not Community)
        let socials_account = Socials {
            twitter: Some("https://x.com/testaccount".to_string()), // Account URL, not Community
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_account), Some(&metadata_valid)).unwrap().is_some(),
            "Token with Twitter Account (not Community) should fail");

        // Test 5: Invalid - Twitter Status (not Community)
        let socials_status = Socials {
            twitter: Some("https://x.com/testaccount/status/123456".to_string()), // Status URL, not Community
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_status), Some(&metadata_valid)).unwrap().is_some(),
            "Token with Twitter Status (not Community) should fail");

        // Test 6: Invalid - No socials at all
        assert!(should_process_token(&config, &accounts, None, Some(&metadata_valid)).unwrap().is_some(),
            "Token with no socials should fail when Twitter Community filter is enabled");

        // Test 7: Valid - ticker length exactly 3 (minimum)
        let metadata_min = TokenMetadata {
            name: "Test Token".to_string(),
            symbol: "ABC".to_string(), // 3 chars, minimum
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_community), Some(&metadata_min)).unwrap().is_none(),
            "Token with ticker length 3 (minimum) should pass");

        // Test 8: Valid - ticker length exactly 7 (maximum)
        let metadata_max = TokenMetadata {
            name: "Test Token".to_string(),
            symbol: "ABCDEFG".to_string(), // 7 chars, maximum
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_community), Some(&metadata_max)).unwrap().is_none(),
            "Token with ticker length 7 (maximum) should pass");
    }

    #[test]
    fn test_detailed_failure_reasons() {
        use crate::socials::TokenMetadata;
        
        let mut config = create_test_config();
        let accounts = create_test_accounts(5.0);
        
        // Test dev buy filter reason
        let accounts_low = create_test_accounts(0.001);
        let result = should_process_token(&config, &accounts_low, None, None).unwrap();
        assert!(result.is_some(), "Should fail with reason");
        let reason = result.unwrap();
        assert!(reason.contains("Dev buy SOL"), "Should mention dev buy SOL");
        
        // Test missing Twitter reason
        config.enable_has_twitter = true;
        let result = should_process_token(&config, &accounts, None, None).unwrap();
        assert!(result.is_some(), "Should fail with reason");
        let reason = result.unwrap();
        assert!(reason.contains("Missing Twitter"), "Should mention missing Twitter");
        assert!(reason.contains("enable_has_twitter"), "Should mention filter name");
        
        // Test symbol length reason - note: basic ticker length filter runs before advanced filters
        config.enable_has_twitter = false;
        config.min_ticker_length = 3;
        config.max_ticker_length = 7;
        config.enable_symbol_3_4 = true;
        let metadata_short = TokenMetadata {
            name: "Test".to_string(),
            symbol: "AB".to_string(), // Too short for min_ticker_length (3)
            description: String::new(),
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        let result = should_process_token(&config, &accounts, None, Some(&metadata_short)).unwrap();
        assert!(result.is_some(), "Should fail with reason");
        let reason = result.unwrap();
        // Basic ticker length filter runs first, so it will fail on that before advanced filter
        assert!(reason.contains("Symbol length"), "Should mention symbol length");
        
        // Test passed case
        config.enable_symbol_3_4 = false;
        let result = should_process_token(&config, &accounts, None, None).unwrap();
        assert!(result.is_none(), "Should pass when no filters are enabled");
    }

    #[test]
    fn test_twitter_community_filter_with_socials() {
        // Test that Twitter Community filter works correctly when socials are fetched
        let mut config = create_test_config();
        let accounts = create_test_accounts(5.0);
        
        // Enable Twitter Community filter
        config.enable_twitter_community = true;
        
        // Test with Twitter Community URL
        let socials_community = Socials {
            twitter: Some("https://x.com/i/communities/1234567890".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        
        // Should pass - has Twitter Community
        let result = should_process_token(&config, &accounts, Some(&socials_community), None).unwrap();
        assert!(result.is_none(), "Should pass with Twitter Community");
        
        // Test with Twitter Account URL (not community)
        let socials_account = Socials {
            twitter: Some("https://x.com/testaccount".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        
        // Should fail - has Twitter but not Community
        let result = should_process_token(&config, &accounts, Some(&socials_account), None).unwrap();
        assert!(result.is_some(), "Should fail when Twitter is not Community");
        let reason = result.unwrap();
        assert!(reason.contains("Twitter is not community type"), "Should mention Twitter Community");
        assert!(reason.contains("enable_twitter_community"), "Should mention filter name");
        
        // Test with no Twitter
        let socials_no_twitter = Socials {
            twitter: None,
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        
        // Should fail - no Twitter
        let result = should_process_token(&config, &accounts, Some(&socials_no_twitter), None).unwrap();
        assert!(result.is_some(), "Should fail when no Twitter");
        let reason = result.unwrap();
        assert!(reason.contains("Twitter is not community type"), "Should mention Twitter Community");
        
        // Test with None socials (fetch failed)
        // Should fail - socials not available
        let result = should_process_token(&config, &accounts, None, None).unwrap();
        assert!(result.is_some(), "Should fail when socials are None");
        let reason = result.unwrap();
        assert!(reason.contains("Twitter is not community type"), "Should mention Twitter Community");
    }

    #[test]
    fn test_socials_fetch_failure_handling() {
        // Test that filters properly handle socials fetch failure
        let mut config = create_test_config();
        let accounts = create_test_accounts(5.0);
        
        // Enable filters that require socials
        config.enable_twitter_community = true;
        config.enable_has_twitter = true;
        config.enable_brand_match_and_twitter = true;
        
        // When socials are None (fetch failed), all these filters should fail gracefully
        let result = should_process_token(&config, &accounts, None, None).unwrap();
        assert!(result.is_some(), "Should fail when socials are None and filters require them");
        let reason = result.unwrap();
        // Should fail on the first filter that requires socials
        assert!(reason.contains("Twitter") || reason.contains("socials"), 
                "Should mention Twitter or socials in failure reason");
    }
}

