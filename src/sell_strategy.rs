// sell_strategy.rs - DYNAMIC SELL STRATEGY SYSTEM
// 
// Ovaj modul implementira fleksibilan sistem za automatsku prodaju tokena sa više strategija.
// Omogućava konfiguraciju više pravila prodaje sa različitim triggerima, prioritetima i ograničenjima.
//
// Kako radi:
// 1. Svako pravilo ima trigger (kada se aktivira), sell_percent (koliko % prodati), i priority (redoslijed provjere)
// 2. Pravila se provjeravaju po prioritetu (viši = prvo)
// 3. Prvo pravilo koje se aktivira se izvršava
// 4. Može se dozvoliti višestruke djelomične prodaje (allow_multiple_sells)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

/// Tipovi triggera za aktivaciju sell pravila
/// Svaki trigger definira uvjet kada se pravilo aktivira
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum SellTrigger {
    /// MarketCapSol: Aktivira se kada market cap dosegne određenu vrijednost u SOL
    /// Vrijednost: market cap u SOL (npr. 175.0 = 175 SOL)
    #[serde(rename = "MarketCapSol")]
    MarketCapSol(f64),
    
    /// StopLoss: Aktivira se kada gubitak dosegne određeni postotak
    /// Vrijednost: postotak gubitka (npr. 30.0 = -30% gubitak)
    #[serde(rename = "StopLoss")]
    StopLoss(f64),
}

/// Pravilo za automatsku prodaju
/// Definira kada i koliko prodati iz pozicije
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SellRule {
    /// Jedinstveni identifikator pravila (npr. "partial_50pct", "stop_loss_30pct")
    pub id: String,
    
    /// Tip triggera i vrijednost - kada se pravilo aktivira
    pub trigger: SellTrigger,
    
    /// Postotak pozicije za prodaju (0.0 - 100.0)
    /// Primjer: 50.0 = prodaj 50% pozicije, 100.0 = prodaj sve
    pub sell_percent: f64,
    
    /// Prioritet provjere (viši broj = provjerava se prvo)
    /// Primjer: 300 = najviši prioritet, 50 = najniži
    /// Pravila se sortiraju po prioritetu prije provjere
    pub priority: i32,
    
    /// Da li je pravilo aktivno (enabled = true) ili ne (enabled = false)
    pub enabled: bool,
    
    /// Opcionalno: Minimalni profit % za aktivaciju pravila
    /// Primjer: Some(30.0) = aktiviraj samo ako je profit >= +30%
    /// None = nema minimalnog profita
    pub min_pnl_percent: Option<f64>,
    
    /// Opcionalno: Maksimalni profit % za aktivaciju pravila
    /// Primjer: Some(200.0) = aktiviraj samo ako je profit <= +200%
    /// None = nema maksimalnog profita
    pub max_pnl_percent: Option<f64>,
    
    /// Opcionalno: Minimalno vrijeme nakon kupnje u sekundama
    /// Primjer: Some(60) = aktiviraj samo ako je prošlo najmanje 60 sekundi od kupnje
    /// None = nema minimalnog vremena
    pub min_time_after_buy: Option<u64>,
    
    /// Opcionalno: Maksimalno vrijeme nakon kupnje u sekundama
    /// Primjer: Some(3600) = aktiviraj samo ako je prošlo najviše 3600 sekundi (1 sat) od kupnje
    /// None = nema maksimalnog vremena
    pub max_time_after_buy: Option<u64>,
}

/// Konfiguracija sell strategije
/// Sadrži listu pravila, globalne postavke i tracking izvršenih pravila
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SellStrategyConfig {
    /// Lista svih sell pravila
    /// Pravila se provjeravaju po prioritetu (viši prioritet = prvo)
    pub rules: Vec<SellRule>,
    
    /// Default postotak prodaje ako nijedno pravilo ne odgovara
    /// Primjer: 100.0 = prodaj sve ako nijedno pravilo ne aktivira
    pub default_sell_percent: f64,
    
    /// Da li dozvoliti višestruke djelomične prodaje
    /// true = može se aktivirati više pravila za istu poziciju (npr. 50% na +50%, zatim 25% na +100%)
    /// false = samo jedno pravilo može se aktivirati po poziciji
    pub allow_multiple_sells: bool,
    
    /// Tracking izvršenih pravila po poziciji (mint -> lista rule_id-jeva)
    /// Koristi se da se ne aktivira isto pravilo više puta (ako allow_multiple_sells = false)
    #[serde(skip)]
    pub executed_rules: HashMap<String, Vec<String>>, // mint -> rule_ids
}

impl SellStrategyConfig {
    /// Kreira jednostavnu default sell strategiju sa 2 osnovna pravila
    /// 
    /// # Default pravila (po prioritetu):
    /// 1. **Stop Loss (-30%)** - Priority 300 - Prodaje sve na -30% gubitka
    /// 2. **Take Profit MC** - Priority 200 - Prodaje sve kada MC dosegne određenu vrijednost (iz Settings)
    pub fn default() -> Self {
        Self {
            rules: vec![
                // Stop Loss at -30% (highest priority)
                // Zaštita od velikih gubitaka - prodaje sve ako gubitak dosegne -30%
                SellRule {
                    id: "stop_loss_30pct".to_string(),
                    trigger: SellTrigger::StopLoss(30.0),
                    sell_percent: 100.0,
                    priority: 300, // Najviši prioritet - provjerava se prvo
                    enabled: true,
                    min_pnl_percent: None,
                    max_pnl_percent: None,
                    min_time_after_buy: None,
                    max_time_after_buy: None,
                },
                // Take profit at MC threshold
                // Potpuna prodaja kada market cap dosegne određenu vrijednost
                SellRule {
                    id: "take_profit_mc".to_string(),
                    trigger: SellTrigger::MarketCapSol(175.0), // Ova vrijednost će biti zamijenjena dinamički iz Settings
                    sell_percent: 100.0,
                    priority: 200, // Prioritet 200 - provjerava se nakon stop loss-a
                    enabled: true,
                    min_pnl_percent: None,
                    max_pnl_percent: None,
                    min_time_after_buy: None,
                    max_time_after_buy: None,
                },
            ],
            default_sell_percent: 100.0, // Ako nijedno pravilo ne odgovara, prodaj sve
            allow_multiple_sells: false, // Jednostavna strategija - nema partial sell-a
            executed_rules: HashMap::new(),
        }
    }

    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Provjerava koje pravilo treba aktivirati za poziciju
    /// 
    /// # Parametri
    /// - `position`: Informacije o poziciji (entry price, MC, itd.)
    /// - `current_pnl_percent`: Trenutni profit/gubitak u postotku (npr. Some(50.0) = +50%)
    /// - `current_mc_sol`: Trenutni market cap u SOL
    /// - `time_since_buy`: Vrijeme u sekundama od kupnje
    /// - `executed_rule_ids`: Lista ID-jeva pravila koja su već izvršena za ovu poziciju
    /// 
    /// # Povratna vrijednost
    /// - `Some(rule)`: Pravilo koje treba aktivirati (prvo koje zadovoljava uvjete)
    /// - `None`: Nijedno pravilo ne zadovoljava uvjete
    /// 
    /// # Kako radi
    /// 1. Sortira pravila po prioritetu (viši prvo)
    /// 2. Provjerava svako enabled pravilo redom
    /// 3. Provjerava time constraints (min/max time after buy)
    /// 4. Provjerava PnL constraints (min/max PnL percent)
    /// 5. Provjerava trigger uvjet (MC threshold ili stop loss)
    /// 6. Vraća prvo pravilo koje zadovoljava sve uvjete
    pub fn check_rules(
        &self,
        _position: &crate::accounts::TokenBuy,
        current_pnl_percent: Option<f64>,
        current_mc_sol: Option<f64>,
        time_since_buy: u64,
        executed_rule_ids: &[String],
    ) -> Option<&SellRule> {
        // Sort rules by priority (higher first)
        let mut sorted_rules: Vec<&SellRule> = self.rules.iter()
            .filter(|r| r.enabled)
            .collect();
        sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        for rule in sorted_rules {
            // Skip if already executed (unless allow_multiple_sells)
            if !self.allow_multiple_sells && executed_rule_ids.contains(&rule.id) {
                continue;
            }

            // Check time constraints
            if let Some(min_time) = rule.min_time_after_buy {
                if time_since_buy < min_time {
                    continue;
                }
            }
            if let Some(max_time) = rule.max_time_after_buy {
                if time_since_buy > max_time {
                    continue;
                }
            }

            // Check PnL constraints
            if let Some(pnl) = current_pnl_percent {
                if let Some(min_pnl) = rule.min_pnl_percent {
                    if pnl < min_pnl {
                        continue;
                    }
                }
                if let Some(max_pnl) = rule.max_pnl_percent {
                    if pnl > max_pnl {
                        continue;
                    }
                }
            }

            // Check trigger condition
            let should_trigger = match &rule.trigger {
                SellTrigger::MarketCapSol(threshold) => {
                    current_mc_sol.map_or(false, |mc| mc >= *threshold)
                }
                SellTrigger::StopLoss(threshold) => {
                    current_pnl_percent.map_or(false, |pnl| pnl <= -*threshold)
                }
            };

            if should_trigger {
                return Some(rule);
            }
        }

        None
    }

    /// Označava pravilo kao izvršeno za poziciju
    /// Koristi se za tracking da se ne aktivira isto pravilo više puta
    pub fn mark_rule_executed(&mut self, mint: &str, rule_id: &str) {
        self.executed_rules
            .entry(mint.to_string())
            .or_insert_with(Vec::new)
            .push(rule_id.to_string());
    }

    /// Provjerava da li je pravilo već izvršeno za poziciju
    /// Vraća true ako je pravilo već izvršeno, false ako nije
    pub fn is_rule_executed(&self, mint: &str, rule_id: &str) -> bool {
        self.executed_rules
            .get(mint)
            .map_or(false, |ids| ids.contains(&rule_id.to_string()))
    }

    /// Vraća listu ID-jeva pravila koja su već izvršena za poziciju
    /// Koristi se za provjeru da se ne aktiviraju već izvršena pravila
    pub fn get_executed_rules(&self, mint: &str) -> Vec<String> {
        self.executed_rules
            .get(mint)
            .cloned()
            .unwrap_or_default()
    }

    /// Creates default strategy with custom MC trigger
    /// Koristi se kada se želi koristiti MC trigger iz Settings umjesto fiksnog 175 SOL
    pub fn default_with_mc_trigger(mc_sol: f64) -> Self {
        let mut config = Self::default();
        // Find and update take_profit_mc rule
        if let Some(rule) = config.rules.iter_mut().find(|r| r.id == "take_profit_mc") {
            if let SellTrigger::MarketCapSol(_) = rule.trigger {
                rule.trigger = SellTrigger::MarketCapSol(mc_sol);
            }
        }
        config
    }
    
    /// Updates MC trigger value for take_profit_mc rule
    /// Koristi se kada se Settings promijeni i treba ažurirati MC trigger u strategiji
    pub fn update_mc_trigger(&mut self, mc_sol: f64) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == "take_profit_mc") {
            if let SellTrigger::MarketCapSol(_) = rule.trigger {
                rule.trigger = SellTrigger::MarketCapSol(mc_sol);
            }
        }
    }
}

