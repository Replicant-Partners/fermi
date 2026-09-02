//! Weather forecasting tools for prediction-market betting.
//!
//! These back the `weather_oracle` composition (see
//! `docs/agents/WEATHER_ORACLE.md`). The design follows the error-source
//! decomposition documented in `docs/WEATHER_MARKETS_RESEARCH.md`:
//!
//! | Error source | Tool that addresses it |
//! |---|---|
//! | Settlement/definitional risk | [`weather_settlement_spec`] — station identity, timezone, unit, rounding |
//! | Model & structural error | [`weather_ensemble_forecast`] — 5-model, ~161-member ensemble |
//! | Long-lead skill collapse | [`weather_climatology`] — ERA5 base rate + warming trend |
//! | Intraday information | [`weather_station_observation`] — running max/min so far today |
//! | Mispricing | [`polymarket_weather_markets`], [`polymarket_orderbook`] |
//!
//! Every endpoint used here is keyless and free. Nothing in this module
//! requires a secret, which is why the agents that declare these tools have
//! an empty `requires_secrets`.
//!
//! **Deliberate omission.** The research brief identified an undocumented
//! Weather Underground backing endpoint that reproduces the exact settlement
//! table for ~44 of the Polymarket series. It is not wired here: the key is
//! scraped from WU's own web client and using it programmatically is very
//! likely outside The Weather Company's terms. It is documented for manual
//! backtesting only. Agents are instructed to treat NWS CLI products (US) and
//! the observation feed as the settlement proxy, and to widen uncertainty for
//! non-US stations where no free settlement-grade feed exists.

use super::tools::BuiltinToolDef;
use serde_json::{json, Value};

const USER_AGENT: &str = "fermi-agent-bestiary/1.0 (weather-oracle; https://github.com/fermi)";
const HTTP_TIMEOUT_SECS: u64 = 25;

// ═══════════════════════════════════════════════════════════════════════════
// Station registry
// ═══════════════════════════════════════════════════════════════════════════

/// Settlement stations for Polymarket daily temperature series.
///
/// Verified against the OurAirports dataset for coordinates/elevation, and
/// against Open-Meteo `timezone=auto` for the IANA zone (both passes agreed
/// for all 50 rows). See `docs/weather_stations_verified.csv`.
///
/// The non-obvious entries are the whole point of this table: Polymarket's
/// NYC *temperature* markets settle on **KLGA**, not Central Park; Dallas on
/// **Love Field**, not DFW; Denver on **Buckley SFB**; Paris on **Le Bourget**;
/// London on **London City**; Seoul on **Incheon**; Taipei on **Songshan**.
/// Forecasting the wrong station is a larger error than any modelling choice.
///
/// (icao, name, municipality, iso2, lat, lon, elevation_m, tz)
type StationRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    f64,
    f64,
    i32,
    &'static str,
);

const STATIONS: &[StationRow] = &[
    (
        "CYYZ",
        "Toronto Pearson International Airport",
        "Toronto",
        "CA",
        43.67594,
        -79.62942,
        173,
        "America/Toronto",
    ),
    (
        "EDDM",
        "Munich Airport",
        "Munich",
        "DE",
        48.35380,
        11.78610,
        453,
        "Europe/Berlin",
    ),
    (
        "EFHK",
        "Helsinki Vantaa Airport",
        "Helsinki",
        "FI",
        60.31836,
        24.96334,
        55,
        "Europe/Helsinki",
    ),
    (
        "EGLC",
        "London City Airport",
        "London",
        "GB",
        51.50530,
        0.05528,
        6,
        "Europe/London",
    ),
    (
        "EHAM",
        "Amsterdam Airport Schiphol",
        "Amsterdam",
        "NL",
        52.30860,
        4.76389,
        -3,
        "Europe/Amsterdam",
    ),
    (
        "EPWA",
        "Warsaw Chopin Airport",
        "Warsaw",
        "PL",
        52.16570,
        20.96710,
        110,
        "Europe/Warsaw",
    ),
    (
        "FACT",
        "Cape Town International Airport",
        "Cape Town",
        "ZA",
        -33.97403,
        18.60433,
        46,
        "Africa/Johannesburg",
    ),
    (
        "HKO",
        "Hong Kong Observatory Headquarters",
        "Hong Kong",
        "HK",
        22.30194,
        114.17417,
        32,
        "Asia/Hong_Kong",
    ),
    (
        "KATL",
        "Hartsfield Jackson Atlanta International Airport",
        "Atlanta",
        "US",
        33.63670,
        -84.42810,
        313,
        "America/New_York",
    ),
    (
        "KAUS",
        "Austin Bergstrom International Airport",
        "Austin",
        "US",
        30.19753,
        -97.66201,
        165,
        "America/Chicago",
    ),
    (
        "KBKF",
        "Buckley Space Force Base",
        "Aurora",
        "US",
        39.70170,
        -104.75200,
        1726,
        "America/Denver",
    ),
    (
        "KDAL",
        "Dallas Love Field",
        "Dallas",
        "US",
        32.84478,
        -96.84765,
        148,
        "America/Chicago",
    ),
    (
        "KLAX",
        "Los Angeles International Airport",
        "Los Angeles",
        "US",
        33.94250,
        -118.40800,
        38,
        "America/Los_Angeles",
    ),
    (
        "KLGA",
        "LaGuardia Airport",
        "New York",
        "US",
        40.77720,
        -73.87260,
        6,
        "America/New_York",
    ),
    (
        "KMIA",
        "Miami International Airport",
        "Miami",
        "US",
        25.79601,
        -80.28975,
        2,
        "America/New_York",
    ),
    (
        "KNYC",
        "New York Central Park",
        "New York",
        "US",
        40.77890,
        -73.96920,
        43,
        "America/New_York",
    ),
    (
        "KORD",
        "Chicago O'Hare International Airport",
        "Chicago",
        "US",
        41.97860,
        -87.90480,
        207,
        "America/Chicago",
    ),
    (
        "KSEA",
        "Seattle-Tacoma International Airport",
        "Seattle",
        "US",
        47.44794,
        -122.31028,
        132,
        "America/Los_Angeles",
    ),
    (
        "KSFO",
        "San Francisco International Airport",
        "San Francisco",
        "US",
        37.61981,
        -122.37482,
        4,
        "America/Los_Angeles",
    ),
    (
        "LEMD",
        "Adolfo Suarez Madrid-Barajas Airport",
        "Madrid",
        "ES",
        40.49341,
        -3.57225,
        609,
        "Europe/Madrid",
    ),
    (
        "LFPB",
        "Paris-Le Bourget International Airport",
        "Paris",
        "FR",
        48.96228,
        2.43654,
        66,
        "Europe/Paris",
    ),
    (
        "LIMC",
        "Milan Malpensa International Airport",
        "Milan",
        "IT",
        45.63060,
        8.72811,
        234,
        "Europe/Rome",
    ),
    (
        "LLBG",
        "Ben Gurion International Airport",
        "Tel Aviv",
        "IL",
        32.01140,
        34.88670,
        41,
        "Asia/Jerusalem",
    ),
    (
        "LTAC",
        "Esenboga International Airport",
        "Ankara",
        "TR",
        40.12810,
        32.99510,
        952,
        "Europe/Istanbul",
    ),
    (
        "LTFM",
        "Istanbul Airport",
        "Istanbul",
        "TR",
        41.27487,
        28.73214,
        99,
        "Europe/Istanbul",
    ),
    (
        "MMMX",
        "Mexico City Benito Juarez International Airport",
        "Mexico City",
        "MX",
        19.43582,
        -99.07033,
        2230,
        "America/Mexico_City",
    ),
    (
        "NZWN",
        "Wellington International Airport",
        "Wellington",
        "NZ",
        -41.32684,
        174.80686,
        12,
        "Pacific/Auckland",
    ),
    (
        "OEJN",
        "King Abdulaziz International Airport",
        "Jeddah",
        "SA",
        21.68024,
        39.15744,
        15,
        "Asia/Riyadh",
    ),
    (
        "OPKC",
        "Jinnah International Airport",
        "Karachi",
        "PK",
        24.90650,
        67.16080,
        30,
        "Asia/Karachi",
    ),
    (
        "RCSS",
        "Taipei Songshan International Airport",
        "Taipei",
        "TW",
        25.06724,
        121.55282,
        5,
        "Asia/Taipei",
    ),
    (
        "RJTT",
        "Tokyo Haneda International Airport",
        "Tokyo",
        "JP",
        35.54968,
        139.78696,
        11,
        "Asia/Tokyo",
    ),
    (
        "RKPK",
        "Gimhae International Airport",
        "Busan",
        "KR",
        35.17950,
        128.93800,
        2,
        "Asia/Seoul",
    ),
    (
        "RKSI",
        "Incheon International Airport",
        "Seoul",
        "KR",
        37.46910,
        126.45100,
        7,
        "Asia/Seoul",
    ),
    (
        "RPLL",
        "Ninoy Aquino International Airport",
        "Manila",
        "PH",
        14.50860,
        121.02000,
        23,
        "Asia/Manila",
    ),
    (
        "SAEZ",
        "Ezeiza International Airport - Ministro Pistarini",
        "Buenos Aires",
        "AR",
        -34.82220,
        -58.53580,
        20,
        "America/Argentina/Buenos_Aires",
    ),
    (
        "SBGR",
        "Sao Paulo-Guarulhos International Airport",
        "Sao Paulo",
        "BR",
        -23.43127,
        -46.46995,
        750,
        "America/Sao_Paulo",
    ),
    (
        "UUWW",
        "Vnukovo International Airport",
        "Moscow",
        "RU",
        55.59150,
        37.26150,
        209,
        "Europe/Moscow",
    ),
    (
        "VILK",
        "Chaudhary Charan Singh International Airport",
        "Lucknow",
        "IN",
        26.76060,
        80.88930,
        125,
        "Asia/Kolkata",
    ),
    (
        "WMKK",
        "Kuala Lumpur International Airport",
        "Kuala Lumpur",
        "MY",
        2.74558,
        101.71000,
        21,
        "Asia/Kuala_Lumpur",
    ),
    (
        "WSSS",
        "Singapore Changi Airport",
        "Singapore",
        "SG",
        1.35019,
        103.99400,
        7,
        "Asia/Singapore",
    ),
    (
        "ZBAA",
        "Beijing Capital International Airport",
        "Beijing",
        "CN",
        40.07735,
        116.59670,
        35,
        "Asia/Shanghai",
    ),
    (
        "ZGGG",
        "Guangzhou Baiyun International Airport",
        "Guangzhou",
        "CN",
        23.39240,
        113.29900,
        15,
        "Asia/Shanghai",
    ),
    (
        "ZGSZ",
        "Shenzhen Bao'an International Airport",
        "Shenzhen",
        "CN",
        22.63947,
        113.80326,
        4,
        "Asia/Shanghai",
    ),
    (
        "ZHCC",
        "Zhengzhou Xinzheng International Airport",
        "Zhengzhou",
        "CN",
        34.52650,
        113.84916,
        151,
        "Asia/Shanghai",
    ),
    (
        "ZHHH",
        "Wuhan Tianhe International Airport",
        "Wuhan",
        "CN",
        30.77480,
        114.21372,
        34,
        "Asia/Shanghai",
    ),
    (
        "ZSJN",
        "Jinan Yaoqiang International Airport",
        "Jinan",
        "CN",
        36.85720,
        117.21600,
        23,
        "Asia/Shanghai",
    ),
    (
        "ZSPD",
        "Shanghai Pudong International Airport",
        "Shanghai",
        "CN",
        31.14340,
        121.80500,
        4,
        "Asia/Shanghai",
    ),
    (
        "ZSQD",
        "Qingdao Jiaodong International Airport",
        "Qingdao",
        "CN",
        36.36195,
        120.08817,
        9,
        "Asia/Shanghai",
    ),
    (
        "ZUCK",
        "Chongqing Jiangbei International Airport",
        "Chongqing",
        "CN",
        29.71225,
        106.65189,
        416,
        "Asia/Shanghai",
    ),
    (
        "ZUUU",
        "Chengdu Shuangliu International Airport",
        "Chengdu",
        "CN",
        30.55826,
        103.94597,
        495,
        "Asia/Shanghai",
    ),
];

/// Maps a Polymarket series slug to its settlement station, unit and source.
///
/// `unit` is the unit the market's buckets are stated in — US cities use
/// whole °F in 2°F buckets, everywhere else whole °C in 1°C buckets. Getting
/// this wrong silently shifts every probability.
///
/// (series_slug_stem, icao, unit, bucket_step, resolution_source)
type SeriesEntry = (&'static str, &'static str, &'static str, f64, &'static str);

const SERIES_MAP: &[SeriesEntry] = &[
    ("nyc", "KLGA", "fahrenheit", 2.0, "weather_underground"),
    ("chicago", "KORD", "fahrenheit", 2.0, "weather_underground"),
    ("dallas", "KDAL", "fahrenheit", 2.0, "weather_underground"),
    ("denver", "KBKF", "fahrenheit", 2.0, "weather_underground"),
    (
        "los-angeles",
        "KLAX",
        "fahrenheit",
        2.0,
        "weather_underground",
    ),
    (
        "san-francisco",
        "KSFO",
        "fahrenheit",
        2.0,
        "weather_underground",
    ),
    ("seattle", "KSEA", "fahrenheit", 2.0, "weather_underground"),
    ("miami", "KMIA", "fahrenheit", 2.0, "weather_underground"),
    ("atlanta", "KATL", "fahrenheit", 2.0, "weather_underground"),
    ("austin", "KAUS", "fahrenheit", 2.0, "weather_underground"),
    ("london", "EGLC", "celsius", 1.0, "weather_underground"),
    ("paris", "LFPB", "celsius", 1.0, "weather_underground"),
    ("amsterdam", "EHAM", "celsius", 1.0, "weather_underground"),
    ("madrid", "LEMD", "celsius", 1.0, "weather_underground"),
    ("milan", "LIMC", "celsius", 1.0, "weather_underground"),
    ("munich", "EDDM", "celsius", 1.0, "weather_underground"),
    ("warsaw", "EPWA", "celsius", 1.0, "weather_underground"),
    ("helsinki", "EFHK", "celsius", 1.0, "weather_underground"),
    ("ankara", "LTAC", "celsius", 1.0, "weather_underground"),
    ("tokyo", "RJTT", "celsius", 1.0, "weather_underground"),
    ("seoul", "RKSI", "celsius", 1.0, "weather_underground"),
    ("busan", "RKPK", "celsius", 1.0, "weather_underground"),
    ("shanghai", "ZSPD", "celsius", 1.0, "weather_underground"),
    ("beijing", "ZBAA", "celsius", 1.0, "weather_underground"),
    ("guangzhou", "ZGGG", "celsius", 1.0, "weather_underground"),
    ("shenzhen", "ZGSZ", "celsius", 1.0, "weather_underground"),
    ("chengdu", "ZUUU", "celsius", 1.0, "weather_underground"),
    ("chongqing", "ZUCK", "celsius", 1.0, "weather_underground"),
    ("wuhan", "ZHHH", "celsius", 1.0, "weather_underground"),
    ("qingdao", "ZSQD", "celsius", 1.0, "weather_underground"),
    ("zhengzhou", "ZHCC", "celsius", 1.0, "weather_underground"),
    ("jinan", "ZSJN", "celsius", 1.0, "weather_underground"),
    ("taipei", "RCSS", "celsius", 1.0, "weather_underground"),
    ("singapore", "WSSS", "celsius", 1.0, "weather_underground"),
    (
        "kuala-lumpur",
        "WMKK",
        "celsius",
        1.0,
        "weather_underground",
    ),
    ("manila", "RPLL", "celsius", 1.0, "weather_underground"),
    ("karachi", "OPKC", "celsius", 1.0, "weather_underground"),
    ("lucknow", "VILK", "celsius", 1.0, "weather_underground"),
    ("jeddah", "OEJN", "celsius", 1.0, "weather_underground"),
    ("toronto", "CYYZ", "celsius", 1.0, "weather_underground"),
    ("mexico-city", "MMMX", "celsius", 1.0, "weather_underground"),
    ("sao-paulo", "SBGR", "celsius", 1.0, "weather_underground"),
    (
        "buenos-aires",
        "SAEZ",
        "celsius",
        1.0,
        "weather_underground",
    ),
    ("cape-town", "FACT", "celsius", 1.0, "weather_underground"),
    ("wellington", "NZWN", "celsius", 1.0, "weather_underground"),
    ("tel-aviv", "LLBG", "celsius", 1.0, "noaa_wrh_timeseries"),
    ("moscow", "UUWW", "celsius", 1.0, "noaa_wrh_timeseries"),
    ("istanbul", "LTFM", "celsius", 1.0, "noaa_wrh_timeseries"),
    ("hong-kong", "HKO", "celsius", 0.1, "hk_observatory"),
];

struct Station {
    icao: &'static str,
    name: &'static str,
    municipality: &'static str,
    iso2: &'static str,
    lat: f64,
    lon: f64,
    elevation_m: i32,
    tz: &'static str,
}

fn station_by_icao(icao: &str) -> Option<Station> {
    let up = icao.to_ascii_uppercase();
    STATIONS.iter().find(|s| s.0 == up).map(|s| Station {
        icao: s.0,
        name: s.1,
        municipality: s.2,
        iso2: s.3,
        lat: s.4,
        lon: s.5,
        elevation_m: s.6,
        tz: s.7,
    })
}

/// Resolve free-text city / series slug to a series-map entry.
///
/// Accepts `"NYC"`, `"New York"`, `"nyc-daily-weather"`,
/// `"highest-temperature-in-nyc-on-august-14-2026"` — matching on the longest
/// stem first so `"kuala-lumpur"` isn't shadowed by a shorter key.
fn resolve_series(query: &str) -> Option<&'static SeriesEntry> {
    let norm = query.to_ascii_lowercase().replace([' ', '_'], "-");

    // Common aliases that don't appear literally in the slug.
    let aliased = match norm.as_str() {
        "new-york" | "new-york-city" | "newyork" => "nyc".to_string(),
        "la" => "los-angeles".to_string(),
        "sf" => "san-francisco".to_string(),
        "hongkong" | "hk" => "hong-kong".to_string(),
        "cdmx" => "mexico-city".to_string(),
        _ => norm.clone(),
    };

    let mut best: Option<&'static SeriesEntry> = None;
    for entry in SERIES_MAP {
        if aliased.contains(entry.0) && best.is_none_or(|b| entry.0.len() > b.0.len()) {
            best = Some(entry);
        }
    }
    best
}

// ═══════════════════════════════════════════════════════════════════════════
// Tool definitions
// ═══════════════════════════════════════════════════════════════════════════

/// Tool definitions appended to the platform builtin list.
///
/// Kept separate from `tools_legacy::builtin_tools()` so the weather stack can
/// be reviewed and evolved independently, but they are plain
/// [`BuiltinToolDef`]s and are dispatched by the same compile-time match — so
/// none of them can become a phantom tool.
pub fn tool_defs() -> Vec<BuiltinToolDef> {
    vec![
        BuiltinToolDef {
            name: "weather_settlement_spec",
            description: "Resolve a weather prediction market to its exact settlement specification: which physical station resolves it, the IANA timezone defining the calendar day, the unit the buckets are stated in, the bucket width, the rounding convention, and the resolution source. ALWAYS CALL THIS FIRST for any weather market question. Local lookup, no network. Encodes the non-obvious station identities that make or break these markets (Polymarket NYC temperature settles on KLGA/LaGuardia, not Central Park; Dallas on Love Field, not DFW; Denver on Buckley SFB; Paris on Le Bourget; London on London City; Seoul on Incheon; Taipei on Songshan).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "City name, Polymarket series slug, or full event slug. Examples: 'NYC', 'New York', 'london-daily-weather', 'highest-temperature-in-tokyo-on-august-14-2026'."
                    },
                    "station": {
                        "type": "string",
                        "description": "ICAO station code, if you already know it (e.g. 'KLGA'). Overrides 'city'."
                    },
                    "variable": {
                        "type": "string",
                        "description": "Which market variable. 'high_temp' and 'low_temp' settle on the airport station; 'precipitation' settles on a DIFFERENT station for some cities (NYC precipitation uses Central Park via NOAA, not LaGuardia).",
                        "enum": ["high_temp", "low_temp", "precipitation"],
                        "default": "high_temp"
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "weather_ensemble_forecast",
            description: "Fetch a multi-model, multi-member weather ensemble from Open-Meteo and reduce it to a predictive distribution for a specific station and target date. Pulls up to ~161 members across 5 independent ensembles (ECMWF IFS 51, ICON-EU 40, GFS 31, GEM Global 21, BOM ACCESS 18) — cross-model spread is the epistemic uncertainty a single-model ensemble structurally cannot see. Returns every member value, ensemble mean/median/std, the spread-skill inputs, per-model medians for disagreement diagnosis, empirical exceedance probabilities for any thresholds you pass, and probabilities for a bucket ladder. Aggregates daily max/min in the STATION'S OWN TIMEZONE and in the MARKET'S OWN UNIT so the numbers are directly comparable to the settlement value. Keyless and free.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "station": {
                        "type": "string",
                        "description": "ICAO code from weather_settlement_spec (e.g. 'KLGA'). Supplies coordinates, timezone and elevation automatically."
                    },
                    "latitude": { "type": "number", "description": "Latitude, if the station is not in the registry." },
                    "longitude": { "type": "number", "description": "Longitude, if the station is not in the registry." },
                    "timezone": { "type": "string", "description": "IANA timezone defining the calendar day (e.g. 'America/New_York'). Required if using raw coordinates." },
                    "target_date": {
                        "type": "string",
                        "description": "Target date in YYYY-MM-DD, interpreted in the station's local timezone. Omit to get all available lead times."
                    },
                    "variable": {
                        "type": "string",
                        "description": "Which daily aggregate to forecast.",
                        "enum": ["temperature_2m_max", "temperature_2m_min", "precipitation_sum", "wind_speed_10m_max"],
                        "default": "temperature_2m_max"
                    },
                    "unit": {
                        "type": "string",
                        "description": "Unit for temperature output — MUST match the market's bucket unit.",
                        "enum": ["celsius", "fahrenheit"],
                        "default": "celsius"
                    },
                    "models": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Ensemble models to include. Default is all five. Options: ecmwf_ifs025, icon_eu, icon_global, gfs025, gfs05, gem_global, bom_access_global_ensemble.",
                        "default": ["ecmwf_ifs025", "icon_eu", "gfs025", "gem_global", "bom_access_global_ensemble"]
                    },
                    "thresholds": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Values at which to compute empirical P(X >= threshold) and P(X < threshold) from the member cloud. For a bucketed market, pass every bucket boundary."
                    },
                    "bucket_edges": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Ordered bucket boundaries for a laddered market. For Polymarket US temp ladders (2F buckets, integer settlement), '86-87F' means settlement in {86,87}, so its edges are 85.5 and 87.5. Returns P(bucket) for each interval plus the two open tails."
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "weather_climatology",
            description: "Compute the climatological base rate for a weather event at a station from the ERA5 reanalysis archive (Open-Meteo, keyless). Pulls the same calendar-day window across N past years, returns the per-year values, the empirical distribution, base rates for any thresholds you pass, a fitted linear warming trend (degrees per decade) and the trend-adjusted base rate for the target year. This is the reference forecast: at long lead times you MUST shrink toward it, and it is the sanity check that catches a broken model (if the ensemble says 2% and climatology says 35% at day 12, the ensemble is wrong, not the climate). Also the denominator for any Brier Skill Score.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "station": { "type": "string", "description": "ICAO code from weather_settlement_spec." },
                    "latitude": { "type": "number" },
                    "longitude": { "type": "number" },
                    "timezone": { "type": "string" },
                    "target_date": {
                        "type": "string",
                        "description": "Target date YYYY-MM-DD. The month-day is what matters; the year sets the trend extrapolation point."
                    },
                    "variable": {
                        "type": "string",
                        "enum": ["temperature_2m_max", "temperature_2m_min", "precipitation_sum", "wind_speed_10m_max"],
                        "default": "temperature_2m_max"
                    },
                    "unit": { "type": "string", "enum": ["celsius", "fahrenheit"], "default": "celsius" },
                    "window_days": {
                        "type": "integer",
                        "description": "Half-width in days around the target month-day, to widen the sample (default 5 => an 11-day window per year).",
                        "default": 5
                    },
                    "years_back": {
                        "type": "integer",
                        "description": "How many years of history (default 30, max 45). ERA5 starts 1940.",
                        "default": 30
                    },
                    "thresholds": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Values at which to compute the historical base rate P(X >= threshold), both raw and detrended."
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "weather_dispersion_fit",
            description: "Measure a station's actual forecast error by lead time, and return FITTED parameters for the bucket-ladder FPL. Replaces guessed calibration priors with measurement. Reconstructs up to 120 days of forecast-versus-outcome pairs from Open-Meteo's previous-runs archive (what the forecast said N days before each date, collapsed to a daily max in the station's own timezone), then reports per-lead sample size, mean bias, MAE and RMSE. THE KEY OUTPUT IS RMSE: for an unbiased forecast the RMSE *is* the standard deviation the predictive distribution should have, so it is the calibration target directly, with no spread-skill-ratio intermediate. Also returns today's ensemble spread per lead, both pooled across models and for a single reference model, and the inflation factor implied against each. CRITICAL AND COUNTERINTUITIVE: those two factors point in OPPOSITE directions at short lead. A pooled multi-model spread already contains between-model disagreement, so it is typically OVER-dispersive at 1-2 days (factor below 1); a single-model spread is UNDER-dispersive as the published literature reports (factor above 1). Applying a single-model-derived inflation to a pooled spread double-counts and over-widens. Keyless and free.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "station": {
                        "type": "string",
                        "description": "ICAO code from weather_settlement_spec, e.g. 'EGLC'. Supplies coordinates and the timezone that defines the calendar day."
                    },
                    "latitude": { "type": "number" },
                    "longitude": { "type": "number" },
                    "timezone": { "type": "string" },
                    "variable": {
                        "type": "string",
                        "description": "Which daily aggregate to verify.",
                        "enum": ["temperature_2m_max", "temperature_2m_min"],
                        "default": "temperature_2m_max"
                    },
                    "unit": { "type": "string", "enum": ["celsius", "fahrenheit"], "default": "celsius" },
                    "days_back": {
                        "type": "integer",
                        "description": "Days of history to verify against (default 120, the archive limit). More is better; below ~30 the RMSE estimate is too noisy to trade on.",
                        "default": 120
                    },
                    "max_lead": {
                        "type": "integer",
                        "description": "Highest lead time to fit, 1-7. The archive exposes previous_day1..previous_day7.",
                        "default": 7
                    },
                    "reference_model": {
                        "type": "string",
                        "description": "Single ensemble model for the under-dispersion comparison (default ecmwf_ifs025).",
                        "default": "ecmwf_ifs025"
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "weather_station_observation",
            description: "Fetch actual observations for a US station from the National Weather Service (api.weather.gov, keyless) — the settlement-grade truth feed. Returns the running maximum and minimum SO FAR TODAY from the ~5-minute observation stream (this is the single largest source of intraday edge: after the diurnal peak has passed, the day's high is nearly determined while the market may still be priced off the morning forecast), plus the latest Climatological Report (CLI) with the official daily max/min, the 1991-2020 normals, and the records. IMPORTANT: CLI products are revised — a preliminary CLI can differ from the final by a full degree — and the tool reports both issuance times so you can tell which you have. US STATIONS ONLY; returns a clear explanation for non-US stations rather than a wrong number.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "station": {
                        "type": "string",
                        "description": "ICAO station code, e.g. 'KLGA'. Must be a US station."
                    },
                    "include_cli": {
                        "type": "boolean",
                        "description": "Fetch and parse the Climatological Report (official daily max/min, normals, records). Default true.",
                        "default": true
                    },
                    "hours_back": {
                        "type": "integer",
                        "description": "How many hours of the observation stream to summarise (default 24).",
                        "default": 24
                    }
                },
                "required": ["station"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "polymarket_weather_markets",
            description: "Read Polymarket weather markets from the public Gamma API (keyless). Without arguments, lists open weather events by 24h volume. With an event slug, returns that event's full rules text (the 'description' field IS the resolution criteria — read it verbatim, it names the station and the source table), every outcome market in the ladder with its CLOB token ids, and current prices. CRITICAL SLUG WARNING: event slugs are year-suffixed, and un-suffixed slugs resolve to the PRIOR year's event whose rules may differ materially (London settled in Fahrenheit in 2025 and Celsius in 2026) — always pass the year-suffixed slug.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "Event slug, year-suffixed, e.g. 'highest-temperature-in-nyc-on-august-14-2026'. Returns full rules + all ladder outcomes."
                    },
                    "tag_slug": {
                        "type": "string",
                        "description": "List events under a tag. Default 'weather'. Note the weather tag is a grab bag that also carries earthquakes, volcanoes and pandemics — filter by series.",
                        "default": "weather"
                    },
                    "series_slug": {
                        "type": "string",
                        "description": "List events in a recurring series, e.g. 'nyc-daily-weather'. More precise than tag_slug."
                    },
                    "limit": { "type": "integer", "description": "Max events to return (default 20).", "default": 20 },
                    "closed": {
                        "type": "boolean",
                        "description": "Set true to retrieve settled events — use this for backtesting against known outcomes. Default false (open markets only).",
                        "default": false
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "polymarket_orderbook",
            description: "Fetch the live CLOB order book for a Polymarket outcome token and convert it into tradeable decision numbers. Returns best bid/ask with depth, midpoint, spread, the implied probability, and — if you pass your own calibrated probability — the fee-adjusted expected value, the Kelly-optimal stake fraction and an explicit verdict. Prices in the raw API are ordered inconveniently (bids ascending, asks descending, best is LAST); this tool normalises that. Fee model is Polymarket's taker fee, 0.05*p*(1-p) per share, which peaks at 2.5% of notional at p=0.5 — large enough to erase most apparent edges, so the tool reports maker and taker cases separately.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "token_id": {
                        "type": "string",
                        "description": "CLOB token id for the outcome you want to price (from polymarket_weather_markets)."
                    },
                    "fair_probability": {
                        "type": "number",
                        "description": "Your calibrated P(outcome) in [0,1]. If supplied, the tool computes edge, fee-adjusted EV and Kelly sizing.",
                        "minimum": 0.0,
                        "maximum": 1.0
                    },
                    "bankroll_usd": {
                        "type": "number",
                        "description": "Bankroll for absolute stake sizing. Default 1000.",
                        "default": 1000.0
                    },
                    "kelly_fraction": {
                        "type": "number",
                        "description": "Fraction of full Kelly to recommend. Default 0.25 — full Kelly on a miscalibrated weather probability is a fast way to lose the bankroll.",
                        "default": 0.25
                    }
                },
                "required": ["token_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
    ]
}

/// Whether this module owns the given tool name.
///
/// Used by the central dispatcher as a match guard so the weather stack can be
/// routed as a group without listing every arm twice.
pub fn handles(name: &str) -> bool {
    matches!(
        name,
        "weather_settlement_spec"
            | "weather_ensemble_forecast"
            | "weather_climatology"
            | "weather_dispersion_fit"
            | "weather_station_observation"
            | "weather_portfolio_risk"
            | "polymarket_weather_markets"
            | "polymarket_orderbook"
    )
}

/// Dispatch a weather tool. Returns `None` if the name isn't ours, so the
/// caller can fall through to the rest of the platform tools.
pub async fn dispatch(name: &str, input: &Value) -> Option<Result<String, String>> {
    match name {
        "weather_settlement_spec" => Some(settlement_spec(input)),
        "weather_ensemble_forecast" => Some(ensemble_forecast(input).await),
        "weather_climatology" => Some(climatology(input).await),
        "weather_dispersion_fit" => Some(dispersion_fit(input).await),
        "weather_station_observation" => Some(station_observation(input).await),
        "weather_portfolio_risk" => Some(portfolio_risk(input).await),
        "polymarket_weather_markets" => Some(polymarket_markets(input).await),
        "polymarket_orderbook" => Some(polymarket_orderbook(input).await),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared helpers
// ═══════════════════════════════════════════════════════════════════════════

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .unwrap_or_default()
}

async fn get_json(url: &str, params: &[(&str, String)]) -> Result<Value, String> {
    let resp = http_client()
        .get(url)
        .query(params)
        .send()
        .await
        .map_err(|e| format!("request to {url} failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("reading response from {url} failed: {e}"))?;

    if !status.is_success() {
        let snippet: String = body.chars().take(400).collect();
        return Err(format!("{url} returned {status}: {snippet}"));
    }

    serde_json::from_str(&body).map_err(|e| {
        let snippet: String = body.chars().take(400).collect();
        format!("could not parse JSON from {url}: {e} (body: {snippet})")
    })
}

fn out(v: Value) -> Result<String, String> {
    serde_json::to_string_pretty(&v).map_err(|e| format!("serialization error: {e}"))
}

/// Resolve coordinates + timezone from either a registry station or raw input.
fn resolve_location(input: &Value) -> Result<(String, f64, f64, String, Option<i32>), String> {
    if let Some(code) = input.get("station").and_then(|v| v.as_str()) {
        if let Some(s) = station_by_icao(code) {
            return Ok((
                s.icao.to_string(),
                s.lat,
                s.lon,
                s.tz.to_string(),
                Some(s.elevation_m),
            ));
        }
        // Unknown station code but explicit coordinates present — allow it.
        if input.get("latitude").is_none() {
            return Err(format!(
                "station '{code}' is not in the settlement registry. Either pass one of the known \
                 ICAO codes (call weather_settlement_spec to list them) or supply latitude, \
                 longitude and timezone explicitly."
            ));
        }
    }

    let lat = input.get("latitude").and_then(|v| v.as_f64()).ok_or(
        "need either 'station' (a registry ICAO code) or 'latitude'/'longitude'/'timezone'",
    )?;
    let lon = input
        .get("longitude")
        .and_then(|v| v.as_f64())
        .ok_or("'longitude' is required when using raw coordinates")?;
    let tz = input
        .get("timezone")
        .and_then(|v| v.as_str())
        .ok_or(
            "'timezone' is required when using raw coordinates — the calendar day that defines a \
             daily max is a LOCAL day, and getting the zone wrong shifts the whole aggregation \
             window",
        )?
        .to_string();

    let label = input
        .get("station")
        .and_then(|v| v.as_str())
        .unwrap_or("custom")
        .to_string();
    Ok((label, lat, lon, tz, None))
}

fn f64_arr(input: &Value, key: &str) -> Vec<f64> {
    input
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
        .unwrap_or_default()
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn std_dev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return f64::NAN;
    }
    let m = mean(xs);
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64).sqrt()
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// Round to a sane number of decimals for reporting.
fn r(x: f64, dp: i32) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    let f = 10f64.powi(dp);
    json!((x * f).round() / f)
}

// ═══════════════════════════════════════════════════════════════════════════
// weather_settlement_spec
// ═══════════════════════════════════════════════════════════════════════════

fn settlement_spec(input: &Value) -> Result<String, String> {
    let variable = input
        .get("variable")
        .and_then(|v| v.as_str())
        .unwrap_or("high_temp");

    // Explicit ICAO wins.
    if let Some(code) = input.get("station").and_then(|v| v.as_str()) {
        let s = station_by_icao(code).ok_or_else(|| {
            format!(
                "unknown station '{code}'. Known settlement stations: {}",
                STATIONS.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")
            )
        })?;
        let series = SERIES_MAP.iter().find(|e| e.1 == s.icao);
        return out(spec_json(&s, series, variable, None));
    }

    let city = input
        .get("city")
        .and_then(|v| v.as_str())
        .ok_or("provide either 'city' (name or Polymarket slug) or 'station' (ICAO code)")?;

    let series =
        resolve_series(city).ok_or_else(|| {
            format!(
            "could not map '{city}' to a known Polymarket weather series. Known series stems: {}. \
             If this is a market for a city not in the registry, read the market's own rules text \
             via polymarket_weather_markets and pass latitude/longitude/timezone directly to \
             weather_ensemble_forecast.",
            SERIES_MAP.iter().map(|e| e.0).collect::<Vec<_>>().join(", ")
        )
        })?;

    let s = station_by_icao(series.1)
        .ok_or_else(|| format!("internal: series maps to unknown station {}", series.1))?;

    out(spec_json(&s, Some(series), variable, Some(city)))
}

fn spec_json(
    s: &Station,
    series: Option<&'static SeriesEntry>,
    variable: &str,
    matched_query: Option<&str>,
) -> Value {
    let is_precip = variable == "precipitation";

    // Precipitation markets switch station for some cities. NYC is the one
    // that actually appears on Polymarket: temperature settles on KLGA but
    // precipitation settles on Central Park via NOAA.
    let (eff_icao, station_override_note) = if is_precip && s.icao == "KLGA" {
        (
            "KNYC",
            Some(
                "PRECIPITATION USES A DIFFERENT STATION. NYC temperature markets settle on KLGA \
                 (LaGuardia) via Weather Underground, but NYC precipitation markets settle on \
                 Central Park (KNYC / GHCN USW00094728) via NOAA, reported to 2 decimal places.",
            ),
        )
    } else if is_precip && s.icao == "EGLC" {
        (
            "EGLL",
            Some(
                "PRECIPITATION USES A DIFFERENT STATION AND A DIFFERENT AGENCY. London temperature \
                 markets settle on EGLC (London City) via Weather Underground, but London \
                 precipitation markets settle on HEATHROW via the Met Office \
                 `heathrowdata.txt` provisional figure, to 1 decimal place. EGLL is not in this \
                 registry; pass Heathrow coordinates (51.4706, -0.4619, Europe/London) explicitly.",
            ),
        )
    } else {
        (s.icao, None)
    };

    let eff = station_by_icao(eff_icao);
    let (lat, lon, tz, elev) = match &eff {
        Some(e) => (e.lat, e.lon, e.tz, e.elevation_m),
        None => (s.lat, s.lon, s.tz, s.elevation_m),
    };

    let unit = series.map(|e| e.2).unwrap_or(if s.iso2 == "US" {
        "fahrenheit"
    } else {
        "celsius"
    });
    let bucket_step = series
        .map(|e| e.3)
        .unwrap_or(if unit == "fahrenheit" { 2.0 } else { 1.0 });
    let source = series.map(|e| e.4).unwrap_or("unknown");

    let om_variable = match variable {
        "low_temp" => "temperature_2m_min",
        "precipitation" => "precipitation_sum",
        _ => "temperature_2m_max",
    };

    let resolution_source_detail = match source {
        "weather_underground" => json!({
            "id": "weather_underground",
            "reads": "the 'Daily Observations' TABLE on wunderground.com/history/daily/{cc}/{city}/{icao}",
            "critical_rule": "The rules explicitly state the Daily Observations table is primary and the 'Day High & Low' summary box is NOT. When they disagree, the table wins.",
            "precision": "whole degrees in the market's unit",
            "free_machine_readable_equivalent": false,
            "proxy_guidance": "No licensed keyless feed reproduces this exactly. For US stations use weather_station_observation (NWS CLI + 5-min obs) as a close proxy and widen uncertainty by the rounding half-step. For non-US stations there is NO settlement-grade free feed; treat the settlement value as the ensemble estimate plus an extra +/- 1 unit of observational uncertainty."
        }),
        "noaa_wrh_timeseries" => json!({
            "id": "noaa_wrh_timeseries",
            "reads": "the highest reading under the 'Temp' column, all times on the day, on the NOAA weather.gov Western Region Headquarters timeseries page for this station",
            "precision": "whole degrees Celsius",
            "free_machine_readable_equivalent": false,
            "proxy_guidance": "Same upstream METAR as Weather Underground for these stations, so the two agreed in backtest. Non-US station: api.weather.gov does NOT serve it."
        }),
        "hk_observatory" => json!({
            "id": "hk_observatory",
            "reads": "'Absolute Daily Max (deg. C)' from the Hong Kong Observatory Daily Extract",
            "precision": "ONE DECIMAL PLACE — unlike every other city in this set, which uses whole degrees. Bucket ladders are therefore 0.1C-resolvable and there is no rounding cushion.",
            "free_machine_readable_equivalent": false,
            "proxy_guidance": "HKO publishes the Daily Extract on its own site. The station is the Observatory HQ in Tsim Sha Tsui, an urban rooftop site, NOT Hong Kong International Airport (VHHH) — expect a persistent warm bias versus the airport and versus any model grid cell."
        }),
        _ => {
            json!({ "id": "unknown", "proxy_guidance": "Read the market's own rules text via polymarket_weather_markets." })
        }
    };

    let mut warnings: Vec<String> = Vec::new();
    if let Some(n) = station_override_note {
        warnings.push(n.to_string());
    }
    if s.icao == "KLGA" && !is_precip {
        warnings.push(
            "KLGA (LaGuardia), NOT Central Park. Central Park routinely differs from LaGuardia by \
             1-3 F, and Central Park is what most published 'New York' forecasts and climate \
             records refer to. Using it would bias every forecast."
                .into(),
        );
    }
    if s.icao == "KDAL" {
        warnings.push(
            "KDAL (Love Field), NOT KDFW. Different station, different daily extremes.".into(),
        );
    }
    if s.icao == "KBKF" {
        warnings.push(
            "KBKF (Buckley Space Force Base, Aurora CO) at 1726 m, NOT Denver International (KDEN) \
             and NOT the Denver city station. A military field; observation cadence can be \
             sparser than a civil ASOS."
                .into(),
        );
    }
    if s.icao == "LFPB" {
        warnings.push(
            "LFPB (Paris-Le Bourget), NOT Charles de Gaulle and NOT Paris-Montsouris.".into(),
        );
    }
    if s.icao == "EGLC" && !is_precip {
        warnings.push(
            "EGLC (London City Airport), NOT Heathrow. London City is a Docklands riverside site; \
             Heathrow typically runs warmer on hot days."
                .into(),
        );
    }
    if s.icao == "RKSI" {
        warnings.push(
            "RKSI (Incheon International), NOT central Seoul. Incheon is a coastal island site \
             ~50 km west of Seoul and is materially cooler than the Seoul urban station in summer \
             and milder in winter."
                .into(),
        );
    }
    if s.icao == "RCSS" {
        warnings.push("RCSS (Taipei Songshan), the in-city airport, NOT Taoyuan (RCTP).".into());
    }
    if s.icao == "RJTT" {
        warnings.push("RJTT (Haneda), NOT Narita and NOT the JMA Tokyo urban station.".into());
    }
    if s.icao == "ZSQD" {
        warnings.push(
            "ZSQD moved: the ICAO code transferred from Liuting to Qingdao Jiaodong in Aug 2021. \
             Pre-2021 archive series under this code describe a site ~35 km away, which corrupts \
             any climatology window that spans the move. Prefer coordinate-based ERA5 climatology \
             (which weather_climatology uses) over station-id archives here."
                .into(),
        );
    }
    if s.icao == "HKO" {
        warnings.push(
            "Hong Kong resolves to 0.1 C, not whole degrees. Do not apply the integer-rounding \
             logic used for every other city."
                .into(),
        );
    }
    if s.iso2 != "US" {
        warnings.push(
            "NON-US STATION: api.weather.gov serves nothing here, so weather_station_observation \
             is unavailable and you have no settlement-grade truth feed. You cannot verify the \
             running max intraday, and you cannot build a station bias correction from official \
             observations. Widen the predictive distribution accordingly and prefer markets where \
             the model spread is wide relative to the bucket width."
                .into(),
        );
    }

    json!({
        "matched_query": matched_query,
        "market_variable": variable,
        "settlement_station": {
            "icao": eff_icao,
            "name": eff.as_ref().map(|e| e.name).unwrap_or(s.name),
            "municipality": eff.as_ref().map(|e| e.municipality).unwrap_or(s.municipality),
            "country": eff.as_ref().map(|e| e.iso2).unwrap_or(s.iso2),
            "latitude": lat,
            "longitude": lon,
            "elevation_m": elev,
            "timezone": tz
        },
        "day_definition": {
            "window": "local calendar day at the station, local midnight to local midnight",
            "timezone": tz,
            "evidence": "Confirmed empirically from the source observation feeds (KLGA spans 04:51Z-03:51Z, RJTT 15:00Z-14:30Z, EGLC 23:20Z-22:50Z) and corroborated by Gamma's game_start_time being local midnight.",
            "warning": "The Gamma `endDate` field is a NOMINAL 12:00Z placeholder, not a trading cutoff and not the measurement window. Do not use it to define the day."
        },
        "units_and_rounding": {
            "market_unit": unit,
            "bucket_step": bucket_step,
            "settlement_precision": if source == "hk_observatory" { "0.1 degree" } else { "whole degree" },
            "bucket_edge_rule": if source == "hk_observatory" {
                "Hong Kong settles to 0.1 C. Bucket edges are the stated values."
            } else {
                "Settlement is a whole-degree integer, so a bucket labelled 'A-B' means the integer is in {A..B}. Convert to continuous edges by taking [A-0.5, B+0.5). A continuous forecast of 87.4 lands in the 86-87 bucket, not 88-89."
            },
            "critical_warning": "NEVER derive the settlement number by unit conversion. For KLGA on 2026-08-12 there were FOUR different defensible 'daily max' values (5-min feed peak 31.0C -> 87.8F -> 88F; hourly METAR peak 30.0C -> 86F; preliminary NWS CLI 86; final NWS CLI 87). The market resolved 86-87F. Read the source's own published integer; treat conversion-derived values as an independent, noisier estimate."
        },
        "resolution_source": resolution_source_detail,
        "revision_risk": {
            "applies_to": "US stations via NWS CLI",
            "observed": "Preliminary CLI for KLGA 2026-08-12 (issued 20:34Z) reported MAX 86; the final CLI (issued 06:17Z next day) reported 87.",
            "implication": "A preliminary official value is not the settlement value. If you are trading on a CLI reading, check its issuance time and discount a same-day preliminary."
        },
        "settlement_timing": {
            "oracle": "UMA optimistic oracle, umaBond 250, customLiveness 900s (15 min)",
            "measured_delay": "45-90 minutes after local midnight (NYC closed 01:16 EDT, London 00:49 BST, Shenzhen 00:41 CST, SF 01:20 PDT)",
            "implication": "There is a deterministic window between the source publishing the day's figure and the market closing, during which the answer is already knowable. This is the least model-dependent edge available in these markets."
        },
        "open_meteo_variable": om_variable,
        "next_step": format!(
            "Call weather_ensemble_forecast with station='{eff_icao}', variable='{om_variable}', unit='{unit}' and the bucket_edges implied by the ladder."
        ),
        "warnings": warnings
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// weather_ensemble_forecast
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_MODELS: &[&str] = &[
    "ecmwf_ifs025",
    "icon_eu",
    "gfs025",
    "gem_global",
    "bom_access_global_ensemble",
];

/// Strip the Open-Meteo per-member key decoration to recover the model id.
///
/// Keys look like `temperature_2m_max_member01_ecmwf_ifs025_ensemble` for
/// perturbed members and `temperature_2m_max_ecmwf_ifs025_ensemble` for the
/// control run. Both must attribute to `ecmwf_ifs025`.
fn model_of_key(key: &str, variable: &str) -> Option<String> {
    let rest = key.strip_prefix(variable)?.trim_start_matches('_');
    let rest = if let Some(after) = rest.strip_prefix("member") {
        // Drop the two-digit member index that follows `member`.
        after
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches('_')
    } else {
        rest
    };
    let model = rest.strip_suffix("_ensemble").unwrap_or(rest);
    if model.is_empty() {
        None
    } else {
        Some(model.to_string())
    }
}

/// Does a requested model id refer to the same model as a returned one?
///
/// Open-Meteo does not echo request ids verbatim: a request for `gfs025` comes
/// back in the response keys as `ncep_gefs025`. Without this, a model that
/// worked perfectly gets reported as missing, which is worse than useless — it
/// tells the agent its ensemble is narrower than it is.
fn model_names_match(requested: &str, returned: &str) -> bool {
    if requested == returned {
        return true;
    }
    let canon = |s: &str| {
        s.replace("ncep_", "")
            .replace("dwd_", "")
            .replace("cmc_", "")
            .replace("gefs", "gfs")
            .replace('_', "")
    };
    canon(requested) == canon(returned)
}

async fn ensemble_forecast(input: &Value) -> Result<String, String> {
    let (label, lat, lon, tz, elev) = resolve_location(input)?;
    let variable = input
        .get("variable")
        .and_then(|v| v.as_str())
        .unwrap_or("temperature_2m_max")
        .to_string();
    let unit = input
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("celsius");

    let models: Vec<String> = input
        .get("models")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_MODELS.iter().map(|s| s.to_string()).collect());

    let params: Vec<(&str, String)> = vec![
        ("latitude", lat.to_string()),
        ("longitude", lon.to_string()),
        ("daily", variable.clone()),
        ("models", models.join(",")),
        ("timezone", tz.clone()),
        ("temperature_unit", unit.to_string()),
        ("precipitation_unit", "mm".to_string()),
        ("wind_speed_unit", "kmh".to_string()),
        ("forecast_days", "16".to_string()),
    ];

    let resp = get_json("https://ensemble-api.open-meteo.com/v1/ensemble", &params).await?;

    let daily = resp
        .get("daily")
        .and_then(|v| v.as_object())
        .ok_or("Open-Meteo returned no 'daily' block")?;
    let times: Vec<String> = daily
        .get("time")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if times.is_empty() {
        return Err("Open-Meteo returned an empty time axis".into());
    }

    // Which lead time are we pricing?
    let (idx, target_date) = match input.get("target_date").and_then(|v| v.as_str()) {
        Some(d) => {
            let i = times.iter().position(|t| t == d).ok_or_else(|| {
                format!(
                    "target_date {d} is outside the ensemble horizon. Available local dates: {} .. {}. \
                     Beyond ~16 days there is no ensemble skill — use weather_climatology instead.",
                    times.first().map(|s| s.as_str()).unwrap_or("?"),
                    times.last().map(|s| s.as_str()).unwrap_or("?")
                )
            })?;
            (i, d.to_string())
        }
        None => (0, times[0].clone()),
    };

    // Collect member values at the target index, grouped by source model.
    let mut members: Vec<f64> = Vec::new();
    let mut by_model: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for (key, series) in daily.iter() {
        if key == "time" || !key.starts_with(&variable) {
            continue;
        }
        let Some(arr) = series.as_array() else {
            continue;
        };
        let Some(v) = arr.get(idx).and_then(|x| x.as_f64()) else {
            continue; // members legitimately run out at long leads
        };
        members.push(v);
        if let Some(m) = model_of_key(key, &variable) {
            by_model.entry(m).or_default().push(v);
        }
    }

    if members.is_empty() {
        return Err(format!(
            "no ensemble members had a value for {target_date}. This usually means the target date \
             is beyond the horizon of every requested model."
        ));
    }

    let mut sorted = members.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = members.len();
    let ens_mean = mean(&members);
    let ens_sd = std_dev(&members);

    // Per-model medians: the epistemic signal. Ensemble spread within one
    // model cannot see the possibility that the whole model is wrong.
    let model_summary: Vec<Value> = by_model
        .iter()
        .map(|(m, vs)| {
            let mut s = vs.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            json!({
                "model": m,
                "n_members": s.len(),
                "median": r(quantile(&s, 0.5), 2),
                "mean": r(mean(vs), 2),
                "std_dev": r(std_dev(vs), 2),
                "min": r(s[0], 2),
                "max": r(s[s.len() - 1], 2)
            })
        })
        .collect();

    // Models can silently return nothing: `icon_eu` has a Europe-only domain,
    // and Open-Meteo renames some ids in the response keys (`gfs025` comes
    // back as `ncep_gefs025`). Dropping 2 of 5 models without saying so would
    // quietly shrink the cross-model spread and make the agent overconfident
    // about exactly the epistemic uncertainty this tool exists to expose.
    let returned: Vec<String> = by_model.keys().cloned().collect();
    let missing: Vec<Value> = models
        .iter()
        .filter(|m| !returned.iter().any(|rk| model_names_match(m, rk)))
        .map(|m| {
            let reason = match m.as_str() {
                "icon_eu" => "ICON-EU is a REGIONAL model covering Europe only; it returns nothing for stations outside that domain. Use icon_global instead for non-European stations.",
                "bom_access_global_ensemble" => "BOM ACCESS did not return values for this station/lead. Its update cadence is slower and coverage is less complete than the other four.",
                _ => "No values returned for this station and lead time. The model may not cover this domain, may not extend to this lead, or may not have completed its cycle yet.",
            };
            json!({ "model": m, "likely_reason": reason })
        })
        .collect();

    let model_medians: Vec<f64> = by_model
        .values()
        .map(|vs| {
            let mut s = vs.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            quantile(&s, 0.5)
        })
        .collect();
    let cross_model_range = if model_medians.len() > 1 {
        model_medians.iter().cloned().fold(f64::MIN, f64::max)
            - model_medians.iter().cloned().fold(f64::MAX, f64::min)
    } else {
        f64::NAN
    };

    // Empirical exceedance at caller-supplied thresholds.
    let thresholds = f64_arr(input, "thresholds");
    let threshold_probs: Vec<Value> = thresholds
        .iter()
        .map(|t| {
            let ge = members.iter().filter(|v| **v >= *t).count();
            let p = ge as f64 / n as f64;
            // Monte Carlo standard error on the member proportion. With ~161
            // members a 3% tail carries ~1.3pp of pure sampling noise, which
            // matters when you are pricing a 1-cent tick.
            let se = (p * (1.0 - p) / n as f64).sqrt();
            json!({
                "threshold": t,
                "p_at_or_above": r(p, 4),
                "p_below": r(1.0 - p, 4),
                "members_at_or_above": ge,
                "monte_carlo_std_error": r(se, 4)
            })
        })
        .collect();

    // Bucket ladder probabilities.
    let edges = f64_arr(input, "bucket_edges");
    let bucket_probs: Vec<Value> = if edges.len() >= 2 {
        let mut e = edges.clone();
        e.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut buckets = Vec::new();
        let below = members.iter().filter(|v| **v < e[0]).count();
        buckets.push(json!({
            "label": format!("< {}", e[0]),
            "lower": Value::Null,
            "upper": e[0],
            "probability": r(below as f64 / n as f64, 4),
            "members": below
        }));
        for w in e.windows(2) {
            let c = members.iter().filter(|v| **v >= w[0] && **v < w[1]).count();
            buckets.push(json!({
                "label": format!("[{}, {})", w[0], w[1]),
                "lower": w[0],
                "upper": w[1],
                "probability": r(c as f64 / n as f64, 4),
                "members": c
            }));
        }
        let above = members.iter().filter(|v| **v >= e[e.len() - 1]).count();
        buckets.push(json!({
            "label": format!(">= {}", e[e.len() - 1]),
            "lower": e[e.len() - 1],
            "upper": Value::Null,
            "probability": r(above as f64 / n as f64, 4),
            "members": above
        }));
        buckets
    } else {
        Vec::new()
    };

    out(json!({
        "station": label,
        "location": {
            "latitude": lat,
            "longitude": lon,
            "timezone": tz,
            "registry_elevation_m": elev,
            "model_grid_elevation_m": resp.get("elevation"),
            "model_grid_latitude": resp.get("latitude"),
            "model_grid_longitude": resp.get("longitude")
        },
        "target_date": target_date,
        "lead_days": idx,
        "variable": variable,
        "unit": if variable.starts_with("temperature") { unit } else if variable.starts_with("precipitation") { "mm" } else { "kmh" },
        "ensemble": {
            "n_members": n,
            "models_requested": models,
            "models_returned": returned.clone(),
            "models_missing": missing,
            "coverage_warning": if by_model.len() < 3 {
                "FEWER THAN 3 MODELS RETURNED. Cross-model disagreement is not measurable with this few, so the epistemic component of your uncertainty is invisible rather than absent. Widen the distribution beyond what the member histogram implies, or decline the market."
            } else {
                "Sufficient model diversity to measure cross-model disagreement."
            },
            "mean": r(ens_mean, 2),
            "median": r(quantile(&sorted, 0.5), 2),
            "std_dev": r(ens_sd, 3),
            "min": r(sorted[0], 2),
            "max": r(sorted[n - 1], 2),
            "p05": r(quantile(&sorted, 0.05), 2),
            "p10": r(quantile(&sorted, 0.10), 2),
            "p25": r(quantile(&sorted, 0.25), 2),
            "p75": r(quantile(&sorted, 0.75), 2),
            "p90": r(quantile(&sorted, 0.90), 2),
            "p95": r(quantile(&sorted, 0.95), 2),
            "members": sorted.iter().map(|v| r(*v, 2)).collect::<Vec<_>>()
        },
        "per_model": model_summary,
        "epistemic_disagreement": {
            "cross_model_median_range": r(cross_model_range, 2),
            "interpretation": "Spread WITHIN a model is aleatoric (chaos). Spread BETWEEN model medians is epistemic — you do not know which model is right, and no single-model ensemble can see it. When cross_model_median_range is large relative to the ensemble std_dev, the raw member histogram is overconfident even after calibration. Widen the no-trade band rather than betting the histogram."
        },
        "threshold_probabilities": threshold_probs,
        "bucket_probabilities": bucket_probs,
        "calibration_required": {
            "status": "THESE ARE RAW MODEL PROBABILITIES. Do not trade them directly.",
            "variance_inflation": {
                "ssr_definition": "SSR = sqrt((M+1)/M) * ensemble_spread / RMSE_of_ensemble_mean",
                "m_correction_factor": r(((n as f64 + 1.0) / n as f64).sqrt(), 4),
                "why": "Every operational and AI ensemble in the published literature is under-dispersive at 1-7 day leads (IFS ENS, GenCast, U-Cast). Under-dispersion systematically UNDERPRICES the tail, which is exactly what a threshold market pays on. Inflate the predictive variance by 1/SSR^2 using an SSR measured on your own station backtest."
            },
            "grid_to_station": "The model grid cell is ~25 km and its terrain elevation (model_grid_elevation_m) may differ from the station's (registry_elevation_m). Apply a residual correction learned as (station_obs - model_forecast), never a model trained on raw observations: with a raw target the learner just relearns the diurnal/seasonal cycle and adds no skill (Gkirmpas et al. 2025 measured ~50% feature importance on time-of-day and ~0% on the spatial features that were supposed to be doing the downscaling).",
            "next_step": "Pass these numbers plus a weather_climatology base rate to the weather_calibrator agent, which applies the bias/spread correction and the lead-time-dependent climatology blend."
        }
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// weather_climatology
// ═══════════════════════════════════════════════════════════════════════════

/// Ordinary-least-squares slope and intercept of `y` on `x`.
fn ols(xs: &[f64], ys: &[f64]) -> Option<(f64, f64)> {
    if xs.len() != ys.len() || xs.len() < 3 {
        return None;
    }
    let mx = mean(xs);
    let my = mean(ys);
    let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    if sxx.abs() < f64::EPSILON {
        return None;
    }
    let sxy: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = sxy / sxx;
    Some((slope, my - slope * mx))
}

async fn climatology(input: &Value) -> Result<String, String> {
    let (label, lat, lon, tz, _elev) = resolve_location(input)?;
    let variable = input
        .get("variable")
        .and_then(|v| v.as_str())
        .unwrap_or("temperature_2m_max")
        .to_string();
    let unit = input
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("celsius");
    let window = input
        .get("window_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(5)
        .clamp(0, 20);
    let years_back = input
        .get("years_back")
        .and_then(|v| v.as_i64())
        .unwrap_or(30)
        .clamp(5, 45);

    let target = input
        .get("target_date")
        .and_then(|v| v.as_str())
        .ok_or("'target_date' (YYYY-MM-DD) is required")?;
    let target_date = chrono::NaiveDate::parse_from_str(target, "%Y-%m-%d")
        .map_err(|e| format!("target_date '{target}' is not YYYY-MM-DD: {e}"))?;

    // ERA5 has a ~5-day ingest latency, so anchor the newest complete year to
    // last year rather than the target year.
    let target_year = target_date
        .format("%Y")
        .to_string()
        .parse::<i32>()
        .unwrap_or(2026);
    let newest_year = target_year - 1;
    let oldest_year = newest_year - (years_back as i32) + 1;

    // One archive call per year, so each request keeps the aggregation window
    // anchored on the station's own local calendar day.
    let mut per_year: Vec<(i32, Vec<f64>)> = Vec::new();
    for year in oldest_year..=newest_year {
        // `with_year` is on the Datelike trait, and it returns None for a
        // Feb 29 target in a non-leap year — fall back to Feb 28 there rather
        // than dropping the year from the sample.
        let anchor = {
            use chrono::Datelike;
            match target_date.with_year(year) {
                Some(d) => d,
                None => chrono::NaiveDate::from_ymd_opt(year, target_date.month(), 28)
                    .ok_or_else(|| format!("cannot construct a date in year {year}"))?,
            }
        };
        let start = anchor - chrono::Duration::days(window);
        let end = anchor + chrono::Duration::days(window);

        let params: Vec<(&str, String)> = vec![
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("start_date", start.format("%Y-%m-%d").to_string()),
            ("end_date", end.format("%Y-%m-%d").to_string()),
            ("daily", variable.clone()),
            ("timezone", tz.clone()),
            ("temperature_unit", unit.to_string()),
            ("precipitation_unit", "mm".to_string()),
            ("wind_speed_unit", "kmh".to_string()),
        ];
        let resp = get_json("https://archive-api.open-meteo.com/v1/archive", &params).await?;
        let vals: Vec<f64> = resp
            .pointer(&format!("/daily/{variable}"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
            .unwrap_or_default();
        if !vals.is_empty() {
            per_year.push((year, vals));
        }
    }

    if per_year.is_empty() {
        return Err("ERA5 archive returned no data for any year in the requested range".into());
    }

    // Pooled sample across the whole window and all years.
    let all: Vec<f64> = per_year
        .iter()
        .flat_map(|(_, v)| v.iter().cloned())
        .collect();
    let mut sorted = all.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Warming trend fitted on the per-year window means, then extrapolated to
    // the target year. This is what makes a 30-year base rate usable in a
    // warming climate: the raw historical frequency understates the current
    // probability of a hot threshold.
    let years_f: Vec<f64> = per_year.iter().map(|(y, _)| *y as f64).collect();
    let year_means: Vec<f64> = per_year.iter().map(|(_, v)| mean(v)).collect();
    let trend = ols(&years_f, &year_means);
    let (slope, intercept) = trend.unwrap_or((0.0, mean(&year_means)));
    let trend_shift = if trend.is_some() {
        slope * (target_year as f64) + intercept - mean(&year_means)
    } else {
        0.0
    };

    let per_year_json: Vec<Value> = per_year
        .iter()
        .map(|(y, v)| {
            let mut s = v.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            json!({
                "year": y,
                "n_days": v.len(),
                "window_mean": r(mean(v), 2),
                "window_max": r(s[s.len() - 1], 2),
                "window_min": r(s[0], 2)
            })
        })
        .collect();

    let thresholds = f64_arr(input, "thresholds");
    let nn = all.len() as f64;
    let base_rates: Vec<Value> = thresholds
        .iter()
        .map(|t| {
            let raw = all.iter().filter(|v| **v >= *t).count() as f64 / nn;
            // Detrended: shift every historical value forward to target-year
            // climate, then recount.
            let adj = all.iter().filter(|v| **v + trend_shift >= *t).count() as f64 / nn;
            json!({
                "threshold": t,
                "raw_base_rate": r(raw, 4),
                "trend_adjusted_base_rate": r(adj, 4),
                "trend_adjustment_pp": r((adj - raw) * 100.0, 2)
            })
        })
        .collect();

    out(json!({
        "station": label,
        "location": { "latitude": lat, "longitude": lon, "timezone": tz },
        "variable": variable,
        "unit": if variable.starts_with("temperature") { unit } else if variable.starts_with("precipitation") { "mm" } else { "kmh" },
        "target_date": target,
        "sample": {
            "years": format!("{oldest_year}-{newest_year}"),
            "n_years": per_year.len(),
            "window_half_width_days": window,
            "n_observations": all.len(),
            "source": "ERA5 reanalysis via Open-Meteo archive API"
        },
        "distribution": {
            "mean": r(mean(&all), 2),
            "median": r(quantile(&sorted, 0.5), 2),
            "std_dev": r(std_dev(&all), 2),
            "min": r(sorted[0], 2),
            "max": r(sorted[sorted.len() - 1], 2),
            "p05": r(quantile(&sorted, 0.05), 2),
            "p10": r(quantile(&sorted, 0.10), 2),
            "p25": r(quantile(&sorted, 0.25), 2),
            "p75": r(quantile(&sorted, 0.75), 2),
            "p90": r(quantile(&sorted, 0.90), 2),
            "p95": r(quantile(&sorted, 0.95), 2)
        },
        "trend": {
            "slope_per_decade": r(slope * 10.0, 3),
            "shift_to_target_year": r(trend_shift, 3),
            "fitted_on": "per-year window means, OLS",
            "caveat": "A linear fit on ~30 noisy annual window means is a weak estimator; treat the slope as an order-of-magnitude correction, not a precise number. It is still better than assuming zero trend, which biases every warm-threshold base rate downward."
        },
        "base_rates": base_rates,
        "per_year": per_year_json,
        "usage_guidance": {
            "as_reference_forecast": "This is the denominator for Brier Skill Score. A model probability with a worse Brier score than this base rate has negative skill and should not be traded.",
            "as_long_lead_anchor": "Beyond ~10 days, shrink the ensemble probability toward the trend-adjusted base rate with a lead-time-dependent weight fitted on backtest. Beyond ~16 days, use the base rate alone.",
            "as_sanity_check": "If the ensemble probability and this base rate disagree wildly at a long lead time, the ensemble is wrong, not the climate.",
            "era5_caveat": "ERA5 is a ~25 km reanalysis, not a station observation. Its values at this grid cell carry a systematic offset from the settlement station's own gauge. Use it for the SHAPE of the distribution and the relative base rate, and correct the LEVEL against actual station observations where you have them."
        }
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// weather_dispersion_fit
// ═══════════════════════════════════════════════════════════════════════════
//
// Turns the calibration layer from assumed into measured.
//
// The insight that makes this cheap: for an unbiased forecast, the RMSE of the
// forecast against the outcome IS the standard deviation the predictive
// distribution should have. So there is no need to estimate a spread-skill
// ratio and then invert it — measure RMSE per lead and use it directly as the
// predictive sd. RMSE measured at the settlement gauge already contains model
// error, cross-model uncertainty AND grid-to-station representativeness error,
// none of which can be separated empirically and none of which need to be.
//
// Why this matters, concretely. The published under-dispersion result
// (SSR ~ 0.85, so inflate variance by ~1.4x) is measured on SINGLE-MODEL
// ensembles — IFS ENS, GenCast, U-Cast. Applying it to a POOLED multi-model
// spread double-counts, because pooling members from models that disagree
// about the level has already added the between-model variance as apparent
// spread. Measured at EGLC over 120 days, lead 1:
//
//     RMSE                         0.91 C   <- the calibration target
//     pooled 4-model spread        1.17 C   -> implied factor 0.78 (DEFLATE)
//     ECMWF IFS spread alone       0.71 C   -> implied factor 1.28 (INFLATE)
//
// Both are correct; they answer different questions. The market's implied sd
// on that date was 0.94 C — within noise of the measured RMSE, which is a
// useful reminder that a liquid weather market is priced off verified forecast
// error and is a strong benchmark rather than a soft target.
//
// The sign also flips with lead: at EGLC the pooled factor runs ~0.78 at lead 1
// and ~1.29 at lead 7. A single inflation constant is wrong in both directions.

/// Collapse an hourly series into per-local-day extremes.
///
/// Keyed on the local date string the API returns, so the aggregation window is
/// the station's own calendar day — the same window the market settles on.
fn hourly_to_daily_extreme(
    times: &[String],
    values: &[Option<f64>],
    want_max: bool,
) -> std::collections::BTreeMap<String, (f64, usize)> {
    let mut out: std::collections::BTreeMap<String, (f64, usize)> =
        std::collections::BTreeMap::new();
    for (i, ts) in times.iter().enumerate() {
        let Some(v) = values.get(i).copied().flatten() else {
            continue;
        };
        let day = ts.chars().take(10).collect::<String>();
        let e = out.entry(day).or_insert((v, 0));
        if (want_max && v > e.0) || (!want_max && v < e.0) {
            e.0 = v;
        }
        e.1 += 1;
    }
    out
}

/// Today's ensemble spread per lead index, for a given model selection.
async fn spread_by_lead(
    lat: f64,
    lon: f64,
    tz: &str,
    unit: &str,
    daily_var: &str,
    models: &str,
    days: usize,
) -> Result<std::collections::BTreeMap<usize, (f64, usize)>, String> {
    let params: Vec<(&str, String)> = vec![
        ("latitude", lat.to_string()),
        ("longitude", lon.to_string()),
        ("daily", daily_var.to_string()),
        ("models", models.to_string()),
        ("timezone", tz.to_string()),
        ("temperature_unit", unit.to_string()),
        ("forecast_days", days.to_string()),
    ];
    let resp = get_json("https://ensemble-api.open-meteo.com/v1/ensemble", &params).await?;
    let daily = resp
        .get("daily")
        .and_then(|v| v.as_object())
        .ok_or("ensemble API returned no daily block")?;
    let n_times = daily
        .get("time")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let mut out = std::collections::BTreeMap::new();
    for i in 0..n_times {
        let vals: Vec<f64> = daily
            .iter()
            .filter(|(k, _)| k.as_str() != "time" && k.starts_with(daily_var))
            .filter_map(|(_, v)| v.as_array().and_then(|a| a.get(i)).and_then(|x| x.as_f64()))
            .collect();
        if vals.len() > 2 {
            out.insert(i, (std_dev(&vals), vals.len()));
        }
    }
    Ok(out)
}

async fn dispersion_fit(input: &Value) -> Result<String, String> {
    let (label, lat, lon, tz, _elev) = resolve_location(input)?;
    let variable = input
        .get("variable")
        .and_then(|v| v.as_str())
        .unwrap_or("temperature_2m_max");
    let want_max = !variable.ends_with("_min");
    let unit = input
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("celsius");
    let days_back = input
        .get("days_back")
        .and_then(|v| v.as_i64())
        .unwrap_or(120)
        .clamp(14, 120);
    let max_lead = input
        .get("max_lead")
        .and_then(|v| v.as_i64())
        .unwrap_or(7)
        .clamp(1, 7) as usize;
    let reference_model = input
        .get("reference_model")
        .and_then(|v| v.as_str())
        .unwrap_or("ecmwf_ifs025");

    // The archive exposes hourly `temperature_2m_previous_dayN`: what the
    // forecast said N days before each valid hour. Daily aggregates are NOT
    // available in this form (the API rejects them), so pull hourly and
    // collapse to the local-day extreme ourselves.
    // Both the max and the min aggregate come from the same hourly series; the
    // `want_max` flag decides which extreme we collapse to. Only temperature is
    // supported here because the archive's `_previous_dayN` hourly variables do
    // not cover precipitation in a form that aggregates cleanly.
    let hourly_var = "temperature_2m";
    let mut fields = vec![hourly_var.to_string()];
    for l in 1..=max_lead {
        fields.push(format!("{hourly_var}_previous_day{l}"));
    }

    let params: Vec<(&str, String)> = vec![
        ("latitude", lat.to_string()),
        ("longitude", lon.to_string()),
        ("hourly", fields.join(",")),
        ("timezone", tz.clone()),
        ("temperature_unit", unit.to_string()),
        ("past_days", days_back.to_string()),
        ("forecast_days", "1".to_string()),
    ];
    let resp = get_json(
        "https://previous-runs-api.open-meteo.com/v1/forecast",
        &params,
    )
    .await?;
    let h = resp
        .get("hourly")
        .and_then(|v| v.as_object())
        .ok_or("previous-runs API returned no hourly block")?;
    let times: Vec<String> = h
        .get("time")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if times.is_empty() {
        return Err("previous-runs API returned an empty time axis".into());
    }

    let col = |name: &str| -> Vec<Option<f64>> {
        h.get(name)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64()).collect())
            .unwrap_or_default()
    };

    // The verifying analysis: the same series, for dates now in the past.
    let actual = hourly_to_daily_extreme(&times, &col(hourly_var), want_max);

    // Require a near-complete day on both sides before scoring it, so a
    // partial first/last day cannot masquerade as a large error.
    const MIN_HOURS: usize = 20;

    let mut per_lead: Vec<Value> = Vec::new();
    let mut fitted: std::collections::BTreeMap<usize, (usize, f64, f64, f64)> =
        std::collections::BTreeMap::new();

    for l in 1..=max_lead {
        let fc = hourly_to_daily_extreme(
            &times,
            &col(&format!("{hourly_var}_previous_day{l}")),
            want_max,
        );
        let mut errs: Vec<f64> = Vec::new();
        for (day, (fv, fh)) in &fc {
            if *fh < MIN_HOURS {
                continue;
            }
            if let Some((av, ah)) = actual.get(day) {
                if *ah >= MIN_HOURS {
                    errs.push(fv - av);
                }
            }
        }
        if errs.len() < 14 {
            per_lead.push(json!({
                "lead_days": l,
                "n": errs.len(),
                "usable": false,
                "note": "fewer than 14 verifying days; not enough to fit"
            }));
            continue;
        }
        let n = errs.len();
        let bias = mean(&errs);
        let mae = mean(&errs.iter().map(|e| e.abs()).collect::<Vec<_>>());
        let rmse = (errs.iter().map(|e| e * e).sum::<f64>() / n as f64).sqrt();
        // SE of a mean and of an sd estimate. The latter is what bounds how
        // tightly the predictive sd itself can be claimed.
        let bias_se = rmse / (n as f64).sqrt();
        let rmse_se = rmse / (2.0 * n as f64).sqrt();

        // RMSE^2 = bias^2 + variance. If the model APPLIES the bias correction
        // — which the bucket-ladder template does, via its station_bias driver
        // — then the sd of what remains is the residual sd, not the RMSE.
        // Using RMSE alongside an active bias driver counts the bias twice and
        // over-widens. At EGLC lead 2 that is 1.43 versus 1.24, a 15% error in
        // the wrong direction.
        let resid_sd = (rmse * rmse - bias * bias).max(0.0).sqrt();
        fitted.insert(l, (n, bias, resid_sd, rmse));

        per_lead.push(json!({
            "lead_days": l,
            "n": n,
            "usable": true,
            "bias_forecast_minus_actual": r(bias, 3),
            "bias_actual_minus_forecast": r(-bias, 3),
            "bias_std_error": r(bias_se, 3),
            "bias_is_significant": bias.abs() > 2.0 * bias_se,
            "mae": r(mae, 3),
            "rmse": r(rmse, 3),
            "rmse_std_error": r(rmse_se, 3),
            "residual_sd_after_bias_correction": r(resid_sd, 3),
            "which_to_use": "rmse if you do NOT apply the bias correction; residual_sd_after_bias_correction if you DO. Never RMSE alongside an active bias driver."
        }));
    }

    // Today's spread per lead, pooled and single-model, for the comparison
    // that resolves the direction-of-correction confusion.
    let horizon = (max_lead + 2).min(16);
    let pooled = spread_by_lead(
        lat,
        lon,
        &tz,
        unit,
        variable,
        "ecmwf_ifs025,icon_global,gfs025,gem_global",
        horizon,
    )
    .await
    .unwrap_or_default();
    let single = spread_by_lead(lat, lon, &tz, unit, variable, reference_model, horizon)
        .await
        .unwrap_or_default();

    let mut comparison: Vec<Value> = Vec::new();
    for (l, (n, _bias, resid_sd, rmse)) in &fitted {
        let sp = pooled.get(l).map(|(s, _)| *s);
        let si = single.get(l).map(|(s, _)| *s);
        comparison.push(json!({
            "lead_days": l,
            "verifying_days": n,
            "target_predictive_sd": r(*resid_sd, 3),
            "rmse_if_no_bias_correction": r(*rmse, 3),
            "pooled_multimodel_spread": sp.map(|s| r(s, 3)),
            "reference_model_spread": si.map(|s| r(s, 3)),
            "implied_factor_vs_pooled": sp.filter(|s| *s > 0.0).map(|s| r(resid_sd / s, 3)),
            "implied_factor_vs_reference": si.filter(|s| *s > 0.0).map(|s| r(resid_sd / s, 3)),
            "pooled_is_over_dispersive": sp.map(|s| *resid_sd < s)
        }));
    }

    // Ready-to-paste FPL params, per lead.
    let mut fpl: Vec<Value> = Vec::new();
    for (l, (n, bias, resid_sd, rmse)) in &fitted {
        let bias_obs_minus_fc = -bias;
        let bias_se = rmse / (*n as f64).sqrt();
        let sd_se = rmse / (2.0 * *n as f64).sqrt();
        fpl.push(json!({
            "lead_days": l,
            "predictive_sd": r(*resid_sd, 3),
            "predictive_sd_factor_p5": r(1.0 - 2.0 * sd_se / resid_sd, 3),
            "predictive_sd_factor_p50": 1.0,
            "predictive_sd_factor_p95": r(1.0 + 2.0 * sd_se / resid_sd, 3),
            "bias_p5": r(bias_obs_minus_fc - 2.0 * bias_se, 3),
            "bias_p50": r(bias_obs_minus_fc, 3),
            "bias_p95": r(bias_obs_minus_fc + 2.0 * bias_se, 3),
            "note": "bias is stated as OBSERVATION MINUS FORECAST, matching the station_bias driver's sign convention"
        }));
    }

    out(json!({
        "station": label,
        "location": { "latitude": lat, "longitude": lon, "timezone": tz },
        "variable": variable,
        "unit": if variable.starts_with("temperature") { unit } else { "mm" },
        "method": {
            "source": "Open-Meteo previous-runs archive: hourly temperature_2m_previous_dayN, collapsed to the local-day extreme",
            "days_requested": days_back,
            "min_hours_per_day": MIN_HOURS,
            "reference_model": reference_model,
            "key_identity": "For an unbiased forecast the RMSE IS the standard deviation the predictive distribution should have. No spread-skill-ratio intermediate is needed, and RMSE measured at the gauge already contains model error, cross-model uncertainty and grid-to-station representativeness error together."
        },
        "per_lead_error": per_lead,
        "dispersion_comparison": comparison,
        "fitted_fpl_params": fpl,
        "how_to_use": {
            "step_1": "Take predictive_sd for your lead. That is the sd your predictive distribution should have — it is the calibration target, not a correction factor.",
            "step_2": "Set the station_bias triple from bias_p5/p50/p95. Ignore a bias whose bias_is_significant is false; it is sampling noise.",
            "step_3": "Do NOT additionally inflate for under-dispersion, and do NOT add a separate cross-model epistemic term. Both are already inside the measured RMSE. Adding them double-counts, which is the specific error that produced a 42%-too-wide distribution on the London case.",
            "step_4": "Re-run this fit per station. Representativeness error is site-specific and does not transfer — DeepMC reports the same brittleness under transfer learning."
        },
        "caveats": [
            "The verifying series is Open-Meteo's own analysis for past dates, not the market's settlement gauge. It is a close proxy at an airport site but not the settlement source, so treat the fitted sd as a floor rather than an exact figure.",
            "Spread is from TODAY's ensemble run while RMSE is from the trailing window, so the implied factors mix a snapshot with an average. Spread varies with regime; the factor is a guide, and predictive_sd is the number to trust.",
            "Leads are limited to 1-7 by the archive. Beyond lead 7, extrapolate with care or fall back to climatology.",
            "RMSE assumes an approximately symmetric error distribution. For precipitation, or for temperature in a strongly skewed regime, prefer quantile-based calibration."
        ]
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// weather_station_observation
// ═══════════════════════════════════════════════════════════════════════════

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// Which calendar day a CLI product is reporting on.
///
/// This matters more than it looks. A CLI issued at 06:17Z on Aug 13 carries
/// the header `...THE LAGUARDIA NY CLIMATE SUMMARY FOR AUGUST 12 2026...` — it
/// is the *previous* day's summary. An agent that reads the newest CLI and
/// assumes it describes today will be a full day out of phase and will price
/// the wrong market.
fn cli_summary_date(text: &str) -> Option<String> {
    let up = text.to_ascii_uppercase();
    let idx = up.find("CLIMATE SUMMARY FOR")?;
    let tail = &up[idx + "CLIMATE SUMMARY FOR".len()..];
    let cleaned: String = tail
        .chars()
        .take_while(|c| *c != '\n')
        .filter(|c| *c != '.')
        .collect();
    let s = cleaned.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// One parsed row of a CLI product's columnar temperature table.
///
/// The layout is:
/// ```text
/// WEATHER ITEM   OBSERVED TIME   RECORD YEAR NORMAL DEPARTURE LAST
///                 VALUE   (LST)  VALUE       VALUE  FROM      YEAR
///   MAXIMUM         87    434 PM  98    2016  85      2       91
/// ```
/// So the single integer a naive parser grabs is the observed value, and the
/// normal and record are further along the same line, not on lines of their
/// own. `MM` is the missing-data marker.
fn cli_row(text: &str, label: &str) -> Option<Value> {
    for line in text.lines() {
        let t = line.trim();
        let up = t.to_ascii_uppercase();
        if !up.starts_with(label) {
            continue;
        }
        let rest = up[label.len()..].trim().to_string();
        let mut toks: Vec<String> = rest.split_whitespace().map(String::from).collect();
        if toks.is_empty() {
            continue;
        }

        let observed: Option<f64> = toks.first().and_then(|s| s.parse::<f64>().ok());

        // Strip the "434 PM" observation time so the remaining numeric tokens
        // line up positionally with the column headers.
        let mut time_of_day: Option<String> = None;
        if let Some(mpos) = toks.iter().position(|t| t == "AM" || t == "PM") {
            if mpos >= 1 {
                time_of_day = Some(format!("{} {}", toks[mpos - 1], toks[mpos]));
                toks.remove(mpos);
                toks.remove(mpos - 1);
            }
        }

        // toks now: [observed, record, record_year, normal, departure, last_year]
        let num = |i: usize| -> Option<f64> {
            toks.get(i).and_then(|s| {
                if s == "MM" || s == "M" {
                    None
                } else {
                    s.parse::<f64>().ok()
                }
            })
        };

        return Some(json!({
            "observed": observed,
            "observed_at_local": time_of_day,
            "record": num(1),
            "record_year": num(2).map(|y| y as i64),
            "normal_1991_2020": num(3),
            "departure_from_normal": num(4),
            "same_day_last_year": num(5)
        }));
    }
    None
}

async fn station_observation(input: &Value) -> Result<String, String> {
    let code = input
        .get("station")
        .and_then(|v| v.as_str())
        .ok_or("'station' (ICAO code) is required")?
        .to_ascii_uppercase();
    let include_cli = input
        .get("include_cli")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let hours_back = input
        .get("hours_back")
        .and_then(|v| v.as_i64())
        .unwrap_or(24)
        .clamp(1, 72);

    let registry = station_by_icao(&code);
    if let Some(s) = &registry {
        if s.iso2 != "US" {
            return out(json!({
                "station": code,
                "available": false,
                "reason": format!(
                    "{} is in {} and api.weather.gov serves US stations only — it returns null for \
                     EGLC, RJTT, LLBG, UUWW, LTFM and every other non-US site.",
                    code, s.iso2
                ),
                "consequence": "You have NO settlement-grade truth feed for this market. You cannot verify the running daily max intraday, and you cannot fit a station bias correction from official observations.",
                "mitigation": [
                    "Use weather_ensemble_forecast plus weather_climatology only, and widen the predictive distribution to account for unverifiable observational error.",
                    "Prefer these markets only when the ensemble spread is wide relative to the bucket width, so the edge does not depend on sub-degree precision.",
                    "For backtesting (not live trading), NOAA NCEI global-hourly ISD carries this station's history at a 2-3 day lag: https://www.ncei.noaa.gov/access/services/data/v1?dataset=global-hourly"
                ]
            }));
        }
    }

    let obs_url = format!("https://api.weather.gov/stations/{code}/observations");
    let start = (chrono::Utc::now() - chrono::Duration::hours(hours_back))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let obs = get_json(&obs_url, &[("start", start.clone())])
        .await
        .map_err(|e| {
            format!(
            "{e}\n\nIf this is a 404, '{code}' is not an api.weather.gov station id (US stations \
             only). Call weather_settlement_spec to check the country."
        )
        })?;

    let features = obs
        .get("features")
        .and_then(|v| v.as_array())
        .ok_or("api.weather.gov returned no 'features' array")?;

    let mut temps_c: Vec<(String, f64)> = Vec::new();
    let mut precip_mm = 0.0f64;
    for f in features {
        let ts = f
            .pointer("/properties/timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(t) = f
            .pointer("/properties/temperature/value")
            .and_then(|v| v.as_f64())
        {
            temps_c.push((ts, t));
        }
        if let Some(p) = f
            .pointer("/properties/precipitationLastHour/value")
            .and_then(|v| v.as_f64())
        {
            precip_mm += p;
        }
    }

    if temps_c.is_empty() {
        return Err(format!(
            "api.weather.gov returned {} observations for {code} but none carried a temperature \
             value in the last {hours_back}h.",
            features.len()
        ));
    }

    let hi = temps_c
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .cloned()
        .unwrap();
    let lo = temps_c
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .cloned()
        .unwrap();
    let latest = temps_c.first().cloned().unwrap();

    let mut result = json!({
        "station": code,
        "station_name": registry.as_ref().map(|s| s.name),
        "timezone": registry.as_ref().map(|s| s.tz),
        "source": "api.weather.gov station observations (~5 minute cadence)",
        "window_hours": hours_back,
        "n_observations": features.len(),
        "n_with_temperature": temps_c.len(),
        "latest": {
            "timestamp_utc": latest.0,
            "temp_c": r(latest.1, 1),
            "temp_f": r(c_to_f(latest.1), 1)
        },
        "running_extremes_in_window": {
            "max_c": r(hi.1, 1),
            "max_f": r(c_to_f(hi.1), 1),
            "max_at_utc": hi.0,
            "min_c": r(lo.1, 1),
            "min_f": r(c_to_f(lo.1), 1),
            "min_at_utc": lo.0,
            "precipitation_sum_mm": r(precip_mm, 2)
        },
        "intraday_edge_note": "The window here is the trailing N hours in UTC, NOT the station's local calendar day. Convert using the station timezone before comparing to a market that settles on the local day. Once the local solar afternoon has passed, the day's maximum is close to determined — that is the highest-confidence, least-model-dependent state available, and it is frequently still mispriced against a stale morning forecast.",
        "unit_conversion_warning": "The Fahrenheit values above are DERIVED by conversion. The settlement source publishes its own integer, and conversion-plus-rounding can land in a different bucket (KLGA 2026-08-12: the 5-minute feed peaked at 31.0C which converts to 87.8F -> 88F, while the market resolved 86-87F). Use the CLI block below, or the market's named source, for the settlement value."
    });

    if include_cli {
        let loc = code.strip_prefix('K').unwrap_or(&code).to_string();
        match get_json(
            "https://api.weather.gov/products",
            &[
                ("type", "CLI".to_string()),
                ("location", loc.clone()),
                ("limit", "2".to_string()),
            ],
        )
        .await
        {
            Ok(list) => {
                let items = list
                    .get("@graph")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut reports = Vec::new();
                for it in items.iter().take(2) {
                    let Some(id) = it.get("id").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let issued = it
                        .get("issuanceTime")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let prod = match get_json(
                        &format!("https://api.weather.gov/products/{id}"),
                        &[],
                    )
                    .await
                    {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let text = prod
                        .get("productText")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    reports.push(json!({
                        "issuance_time_utc": issued,
                        "summary_is_for_date": cli_summary_date(&text),
                        "maximum": cli_row(&text, "MAXIMUM"),
                        "minimum": cli_row(&text, "MINIMUM"),
                        "average": cli_row(&text, "AVERAGE"),
                        "raw_text": text.chars().take(2600).collect::<String>()
                    }));
                }
                result["climatological_reports"] = json!({
                    "unit": "degrees Fahrenheit (CLI products are always F)",
                    "reports_newest_first": reports,
                    "date_warning": "READ 'summary_is_for_date'. A CLI issued in the small hours reports on the PREVIOUS calendar day, so the newest product is usually not today. Attributing it to today puts you a full day out of phase.",
                    "revision_warning": "CLI products ARE REVISED. Compare issuance times for the SAME summary date: a report issued during the target day is PRELIMINARY; the final is issued after local midnight. For KLGA's 2026-08-12 summary the preliminary said MAXIMUM 86 and the final said 87 — a full degree, enough to move a 2F bucket. If only a preliminary exists, do not treat it as settled.",
                    "bonus": "Each row also carries the 1991-2020 normal, the standing record and its year, and the same day last year — a free, station-exact climatology cross-check on weather_climatology's ERA5 numbers."
                });
            }
            Err(e) => {
                result["climatological_reports"] = json!({
                    "available": false,
                    "error": e,
                    "note": "Not every station issues CLI products; the location code is usually the ICAO minus the leading 'K'."
                });
            }
        }
    }

    out(result)
}

// ═══════════════════════════════════════════════════════════════════════════
// weather_portfolio_risk
// ═══════════════════════════════════════════════════════════════════════════
//
// Two distinct correlation problems live in a weather portfolio, and the
// intuitive one turns out to be the smaller.
//
// ## 1. Across stations — measured, and weaker than expected
//
// The intuition is that a European heatwave makes London, Paris, Amsterdam and
// Munich one bet rather than four. That confuses correlated WEATHER with
// correlated FORECAST ERROR. The weather is indeed highly correlated; the
// errors largely are not, because the models resolve the synoptic pattern for
// all of them and what remains is local. Measured at lead 2 over 120 days:
//
//     EGLC LFPB EHAM EDDM KLGA KLAX  pairwise error correlation 0.05 - 0.25
//     N_eff = N^2 / sum(rho) = 36 / 8.51 = 4.23 of a naive 6
//     Kelly haircut = sqrt(N_eff/N) = 0.84
//
// So cross-station diversification is real but the haircut is mild. An earlier
// hand-waved estimate of "over-levers by sqrt(10)" was simply wrong, which is
// the argument for measuring rather than reasoning about it.
//
// ## 2. Within a ladder — the one that actually matters
//
// A ladder's buckets are MUTUALLY EXCLUSIVE: exactly one resolves YES. Holding
// three adjacent buckets is not three bets, it is one bet on where the centre
// of the distribution sits. Per-bucket Kelly, summed, both over-stakes and
// mis-allocates, because it ignores that the stakes on losing buckets are lost
// with certainty whenever any sibling wins.
//
// The correct treatment is multi-outcome Kelly: choose fractions f_i to
// maximise the expected log growth
//
//     G(f) = sum_i q_i * log(1 - sum_j f_j + f_i / p_i)
//
// which is what `ladder_kelly` solves. It routinely concentrates the stake on
// far fewer buckets than per-bucket Kelly would suggest, and its total stake is
// smaller.
//
// A further shared exposure worth naming: every bucket in a ladder is priced
// off ONE centre estimate. If that centre is biased, all buckets in the ladder
// are wrong together. Cross-station error correlation does not capture it,
// because it is a single common factor within the ladder.

/// Pearson correlation.
fn corr(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.len() < 3 {
        return f64::NAN;
    }
    let (ma, mb) = (mean(a), mean(b));
    let num: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let da: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let db: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    if da <= 0.0 || db <= 0.0 {
        return f64::NAN;
    }
    num / (da * db).sqrt()
}

/// Daily forecast-error series for one station at one lead.
async fn error_series(
    lat: f64,
    lon: f64,
    tz: &str,
    unit: &str,
    lead: usize,
    days_back: i64,
) -> Result<std::collections::BTreeMap<String, f64>, String> {
    let field = format!("temperature_2m_previous_day{lead}");
    let params: Vec<(&str, String)> = vec![
        ("latitude", lat.to_string()),
        ("longitude", lon.to_string()),
        ("hourly", format!("temperature_2m,{field}")),
        ("timezone", tz.to_string()),
        ("temperature_unit", unit.to_string()),
        ("past_days", days_back.to_string()),
        ("forecast_days", "1".to_string()),
    ];
    let resp = get_json(
        "https://previous-runs-api.open-meteo.com/v1/forecast",
        &params,
    )
    .await?;
    let h = resp
        .get("hourly")
        .and_then(|v| v.as_object())
        .ok_or("previous-runs API returned no hourly block")?;
    let times: Vec<String> = h
        .get("time")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let col = |n: &str| -> Vec<Option<f64>> {
        h.get(n)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64()).collect())
            .unwrap_or_default()
    };
    let act = hourly_to_daily_extreme(&times, &col("temperature_2m"), true);
    let fc = hourly_to_daily_extreme(&times, &col(&field), true);

    let mut out = std::collections::BTreeMap::new();
    for (day, (fv, fh)) in &fc {
        if *fh < 20 {
            continue;
        }
        if let Some((av, ah)) = act.get(day) {
            if *ah >= 20 {
                out.insert(day.clone(), fv - av);
            }
        }
    }
    Ok(out)
}

/// Multi-outcome Kelly over a set of mutually exclusive outcomes.
///
/// Maximises `sum_i q_i * log(1 - sum_j f_j + f_i / p_i)` by projected gradient
/// ascent. Returns the stake fraction per outcome. Exact enough for sizing —
/// the objective is concave, so ascent converges to the global optimum, and any
/// residual imprecision is far below the uncertainty in `q`.
fn ladder_kelly(q: &[f64], p: &[f64]) -> Vec<f64> {
    let n = q.len();
    let mut f = vec![0.0f64; n];
    let mut step = 0.02;
    for _ in 0..20_000 {
        let total: f64 = f.iter().sum();
        // Gradient of G wrt f_k: sum_i q_i * (d/df_k wealth_i) / wealth_i
        // wealth_i = 1 - total + f_i/p_i, so d/df_k = -1 + [i==k]/p_k
        let mut grad = vec![0.0f64; n];
        let mut ok = true;
        let wealth: Vec<f64> = (0..n).map(|i| 1.0 - total + f[i] / p[i]).collect();
        for w in &wealth {
            if *w <= 1e-9 {
                ok = false;
            }
        }
        if !ok {
            // Stepped into infeasible territory; pull back and shrink.
            for v in f.iter_mut() {
                *v *= 0.5;
            }
            step *= 0.5;
            continue;
        }
        for k in 0..n {
            let mut g = 0.0;
            for i in 0..n {
                let d = if i == k { 1.0 / p[k] - 1.0 } else { -1.0 };
                g += q[i] * d / wealth[i];
            }
            grad[k] = g;
        }
        for k in 0..n {
            f[k] = (f[k] + step * grad[k]).max(0.0);
        }
        // Keep the total staked strictly inside the simplex.
        let t: f64 = f.iter().sum();
        if t > 0.95 {
            let s = 0.95 / t;
            for v in f.iter_mut() {
                *v *= s;
            }
        }
        step *= 0.9997;
    }
    f
}

fn log_growth(f: &[f64], q: &[f64], p: &[f64]) -> f64 {
    let total: f64 = f.iter().sum();
    (0..q.len())
        .map(|i| {
            let w = 1.0 - total + f[i] / p[i];
            if w <= 0.0 {
                -1e9
            } else {
                q[i] * w.ln()
            }
        })
        .sum()
}

async fn portfolio_risk(input: &Value) -> Result<String, String> {
    let lead = input
        .get("lead_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(2)
        .clamp(1, 7) as usize;
    let days_back = input
        .get("days_back")
        .and_then(|v| v.as_i64())
        .unwrap_or(120)
        .clamp(30, 120);
    let unit = input
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("celsius");

    // ── Part 1: cross-station error correlation ──────────────────────────
    let stations: Vec<String> = input
        .get("stations")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut cross = Value::Null;
    if stations.len() >= 2 {
        let mut series: Vec<(String, std::collections::BTreeMap<String, f64>)> = Vec::new();
        let mut rmse: Vec<Value> = Vec::new();
        for code in &stations {
            let Some(s) = station_by_icao(code) else {
                return Err(format!(
                    "unknown station '{code}'; call weather_settlement_spec for valid codes"
                ));
            };
            let e = error_series(s.lat, s.lon, s.tz, unit, lead, days_back).await?;
            let n = e.len() as f64;
            let station_rmse = if n > 0.0 {
                (e.values().map(|x| x * x).sum::<f64>() / n).sqrt()
            } else {
                f64::NAN
            };
            rmse.push(json!({ "station": s.icao, "n_days": e.len(), "rmse": r(station_rmse, 3) }));
            series.push((s.icao.to_string(), e));
        }

        // Restrict to days every station has, so every correlation is computed
        // on the same sample.
        let mut common: Option<std::collections::BTreeSet<String>> = None;
        for (_, e) in &series {
            let keys: std::collections::BTreeSet<String> = e.keys().cloned().collect();
            common = Some(match common {
                None => keys,
                Some(c) => c.intersection(&keys).cloned().collect(),
            });
        }
        let common: Vec<String> = common.unwrap_or_default().into_iter().collect();

        let vecs: Vec<Vec<f64>> = series
            .iter()
            .map(|(_, e)| common.iter().map(|d| e[d]).collect())
            .collect();

        let n = series.len();
        let mut matrix: Vec<Value> = Vec::new();
        let mut sum_rho = 0.0;
        let mut pairs: Vec<(String, String, f64)> = Vec::new();
        for i in 0..n {
            let row: Vec<Value> = (0..n)
                .map(|j| {
                    let c = if i == j {
                        1.0
                    } else {
                        corr(&vecs[i], &vecs[j])
                    };
                    sum_rho += if c.is_finite() { c } else { 0.0 };
                    if i < j && c.is_finite() {
                        pairs.push((series[i].0.clone(), series[j].0.clone(), c));
                    }
                    r(c, 3)
                })
                .collect();
            matrix.push(json!({ "station": series[i].0, "row": row }));
        }
        pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Effective number of independent bets. For equal-variance positions,
        // portfolio variance scales with the sum of the correlation matrix, so
        // N_eff = N^2 / sum(rho) and the stake haircut is sqrt(N_eff / N).
        let n_eff = if sum_rho > 0.0 {
            (n * n) as f64 / sum_rho
        } else {
            n as f64
        };
        let haircut = (n_eff / n as f64).sqrt().min(1.0);

        // A station whose own error is far larger than its peers is a sizing
        // hazard on its own, independent of correlation.
        let rmses: Vec<f64> = rmse.iter().filter_map(|row| row["rmse"].as_f64()).collect();
        let med = {
            let mut s = rmses.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            quantile(&s, 0.5)
        };
        let outliers: Vec<Value> = rmse
            .iter()
            .filter(|row| row["rmse"].as_f64().is_some_and(|v| v > 1.8 * med))
            .map(|row| {
                json!({
                    "station": row["station"],
                    "rmse": row["rmse"],
                    "median_rmse": r(med, 3),
                    "note": "forecast error far above peers — size this station down independently of correlation, and check whether its grid cell sits over water or terrain the model resolves poorly"
                })
            })
            .collect();

        cross = json!({
            "lead_days": lead,
            "common_days": common.len(),
            "per_station_rmse": rmse,
            "correlation_matrix": matrix,
            "most_correlated_pairs": pairs.iter().take(5).map(|(a, b, c)| json!({
                "a": a, "b": b, "correlation": r(*c, 3)
            })).collect::<Vec<_>>(),
            "sum_of_correlation_matrix": r(sum_rho, 3),
            "naive_independent_bets": n,
            "effective_independent_bets": r(n_eff, 2),
            "kelly_stake_haircut": r(haircut, 3),
            "variance_overstatement_if_ignored": r(n as f64 / n_eff, 2),
            "rmse_outliers": outliers,
            "interpretation": "This measures correlated FORECAST ERROR, not correlated weather. The two differ a great deal: models resolve the shared synoptic pattern for all stations, so what remains is largely local. Cross-station error correlation is typically 0.05-0.25 and the resulting haircut is mild — much milder than reasoning from 'a heatwave hits them all' would suggest."
        });
    }

    // ── Part 2: within-ladder multi-outcome Kelly ────────────────────────
    let mut ladder = Value::Null;
    if let Some(items) = input.get("ladder").and_then(|v| v.as_array()) {
        let labels: Vec<String> = items
            .iter()
            .map(|i| i["label"].as_str().unwrap_or("?").to_string())
            .collect();
        let q: Vec<f64> = items.iter().map(|i| f64_of(&i["model_prob"])).collect();
        let p: Vec<f64> = items.iter().map(|i| f64_of(&i["price"])).collect();

        if q.iter().any(|x| !x.is_finite()) || p.iter().any(|x| !(0.0..1.0).contains(x)) {
            return Err("each ladder entry needs a finite model_prob and a price in (0,1)".into());
        }
        let q_sum: f64 = q.iter().sum();
        let p_sum: f64 = p.iter().sum();
        if q_sum > 1.02 {
            return Err(format!(
                "model probabilities sum to {q_sum:.3}. Ladder outcomes are mutually exclusive, \
                 so they cannot exceed 1 — you have almost certainly read bucket labels as \
                 one-sided thresholds instead of intervals."
            ));
        }
        // A PARTIAL ladder manufactures a phantom arbitrage and the optimiser
        // will happily lever into it. Submitting the 5 central buckets of an
        // 11-bucket London ladder gave prices summing to 0.965 against model
        // probabilities summing to 0.988 — an apparent 2.3% riskless edge that
        // exists only because the omitted buckets cannot lose. Multi-outcome
        // Kelly then staked 95% of bankroll, including on buckets with clearly
        // negative edge.
        if q_sum < 0.95 {
            return Err(format!(
                "model probabilities sum to only {q_sum:.3}, so this ladder is INCOMPLETE. \
                 Multi-outcome Kelly requires the full mutually-exclusive set: with outcomes \
                 missing, the omitted mass looks like free money and the optimiser levers into \
                 a phantom arbitrage. Pass every bucket including the open tails ('N or below', \
                 'N or higher'), or add a residual entry covering the remainder."
            ));
        }
        if p_sum < 0.97 {
            return Err(format!(
                "prices sum to only {p_sum:.3}. Either the ladder is incomplete or there is a \
                 genuine structural arbitrage — buy every outcome for {p_sum:.3} and collect 1.00. \
                 Check completeness first; a real negRisk ladder normally sums slightly ABOVE 1 \
                 because of the spread and taker fee."
            ));
        }

        let f_opt = ladder_kelly(&q, &p);
        let g_opt = log_growth(&f_opt, &q, &p);

        // What per-bucket Kelly would have said, treating each as independent.
        let f_naive: Vec<f64> = q
            .iter()
            .zip(&p)
            .map(|(qi, pi)| ((qi - pi) / (1.0 - pi)).max(0.0))
            .collect();
        let g_naive = log_growth(&f_naive, &q, &p);

        let total_opt: f64 = f_opt.iter().sum();
        let total_naive: f64 = f_naive.iter().sum();

        ladder = json!({
            "outcomes": labels.iter().enumerate().map(|(i, l)| json!({
                "label": l,
                "model_prob": r(q[i], 4),
                "price": r(p[i], 4),
                "edge": r(q[i] - p[i], 4),
                "multi_outcome_kelly_fraction": r(f_opt[i], 4),
                "per_bucket_kelly_fraction": r(f_naive[i], 4)
            })).collect::<Vec<_>>(),
            "model_prob_sum": r(q_sum, 4),
            "price_sum": r(p_sum, 4),
            "ladder_overround": r(p_sum - 1.0, 4),
            "total_stake_multi_outcome": r(total_opt, 4),
            "total_stake_per_bucket_naive": r(total_naive, 4),
            "naive_over_stakes_by": if total_opt > 1e-9 { r(total_naive / total_opt, 2) } else { Value::Null },
            "hit_stake_cap": total_opt > 0.94,
            "log_growth_multi_outcome": r(g_opt, 6),
            "log_growth_per_bucket_naive": r(g_naive, 6),
            "naive_is_worse_by": r(g_opt - g_naive, 6),
            "why": "Ladder buckets are mutually exclusive: exactly one resolves YES, so holding several is ONE bet on where the centre sits, not several independent bets. Per-bucket Kelly ignores that a stake on every losing sibling is lost with certainty whenever any sibling wins, so it over-stakes and mis-allocates. Multi-outcome Kelly maximises expected log growth over the joint outcome and typically concentrates on fewer buckets.",
            "shared_centre_warning": "Every bucket in a ladder is priced off ONE centre estimate. If that centre is biased, all of them are wrong together. Cross-station correlation cannot see this — it is a common factor inside the ladder. Before sizing, check the ensemble centre against the market's implied centre; a gap larger than the measured predictive sd means you are betting on the centre, not on the bucket."
        });
    }

    out(json!({
        "cross_station": cross,
        "within_ladder": ladder,
        "sizing_order": [
            "1. Size each ladder with multi-outcome Kelly, never by summing per-bucket Kelly.",
            "2. Apply the cross-station kelly_stake_haircut across ladders.",
            "3. Cap by order-book depth at your limit price — walking the book converts edge into slippage.",
            "4. Apply a fractional-Kelly multiplier (0.25 default) on top, because none of this is worth anything if the probabilities are not calibrated."
        ],
        "caveats": [
            "Correlations are measured on Open-Meteo's analysis as the verifying truth, not on the market's settlement gauge.",
            "A 120-day window at one lead is a single season. Error correlation is regime-dependent and will differ in winter.",
            "N_eff assumes comparable position variance across stations. Check rmse_outliers before treating the haircut as uniform."
        ]
    }))
}

/// Numeric coercion that accepts both JSON numbers and numeric strings.
fn f64_of(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(f64::NAN)
}

// ═══════════════════════════════════════════════════════════════════════════
// Polymarket
// ═══════════════════════════════════════════════════════════════════════════

const GAMMA: &str = "https://gamma-api.polymarket.com";
const CLOB: &str = "https://clob.polymarket.com";

/// Gamma returns some list-valued fields as JSON-encoded strings.
fn parse_embedded_json(v: Option<&Value>) -> Value {
    match v {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

async fn polymarket_markets(input: &Value) -> Result<String, String> {
    // Single-event mode: return the rules verbatim plus the whole ladder.
    if let Some(slug) = input.get("slug").and_then(|v| v.as_str()) {
        let events = get_json(&format!("{GAMMA}/events"), &[("slug", slug.to_string())]).await?;
        let ev = events
            .as_array()
            .and_then(|a| a.first())
            .ok_or_else(|| {
                format!(
                    "no event with slug '{slug}'. Event slugs are YEAR-SUFFIXED — \
                     'highest-temperature-in-nyc-on-august-14' resolves to the 2025 event, \
                     'highest-temperature-in-nyc-on-august-14-2026' to this year's. Try adding the year."
                )
            })?
            .clone();

        let markets = ev
            .get("markets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let outcomes: Vec<Value> = markets
            .iter()
            .map(|m| {
                json!({
                    "question": m.get("question"),
                    "slug": m.get("slug"),
                    "condition_id": m.get("conditionId"),
                    "clob_token_ids": parse_embedded_json(m.get("clobTokenIds")),
                    "outcomes": parse_embedded_json(m.get("outcomes")),
                    "outcome_prices": parse_embedded_json(m.get("outcomePrices")),
                    "best_bid": m.get("bestBid"),
                    "best_ask": m.get("bestAsk"),
                    "last_trade_price": m.get("lastTradePrice"),
                    "volume_24hr": m.get("volume24hr"),
                    "liquidity": m.get("liquidity"),
                    "order_price_min_tick_size": m.get("orderPriceMinTickSize"),
                    "order_min_size": m.get("orderMinSize"),
                    "closed": m.get("closed"),
                    "neg_risk": m.get("negRisk")
                })
            })
            .collect();

        return out(json!({
            "event": {
                "title": ev.get("title"),
                "slug": ev.get("slug"),
                "closed": ev.get("closed"),
                "start_date": ev.get("startDate"),
                "end_date": ev.get("endDate"),
                "game_start_time": ev.get("gameStartTime"),
                "volume_24hr": ev.get("volume24hr"),
                "liquidity": ev.get("liquidity"),
                "neg_risk": ev.get("negRisk")
            },
            "resolution_criteria_verbatim": ev.get("description"),
            "how_to_read_this": "The 'description' field IS the binding resolution text. Read it literally. It names the station, the source page, and often which TABLE on that page is primary. Cross-check what it says against weather_settlement_spec; if they disagree, the market text wins and the registry needs updating.",
            "outcome_count": outcomes.len(),
            "outcomes": outcomes,
            "day_window_hint": {
                "game_start_time": ev.get("gameStartTime"),
                "note": "gameStartTime is local midnight at the settlement station and is the best machine-readable signal for the measurement window. endDate is a nominal 12:00Z placeholder — it is NOT a trading deadline and NOT the measurement window."
            },
            "microstructure": {
                "structure": "Binary YES/NO conditional tokens on Polygon, USDC collateral, priced 0-1. A bucket ladder is a negRisk mutually-exclusive group, not a native multi-outcome book, so YES prices across the ladder are arbitrage-linked to sum to about 1.",
                "taker_fee": "0.05 * p * (1-p) per share — 2.5% of notional at p=0.5. Maker rebate 25%. Post, do not take.",
                "settlement": "UMA optimistic oracle, ~15 min liveness; observed close 45-90 min after local midnight."
            },
            "next_step": "Pick the outcome you want and pass its clob_token_ids[0] (the YES token) to polymarket_orderbook together with your calibrated probability."
        }));
    }

    // List mode.
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .clamp(1, 100);
    let closed = input
        .get("closed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut params: Vec<(&str, String)> = vec![
        ("limit", limit.to_string()),
        ("closed", closed.to_string()),
        ("order", "volume24hr".to_string()),
        ("ascending", "false".to_string()),
    ];
    if let Some(series) = input.get("series_slug").and_then(|v| v.as_str()) {
        params.push(("series_slug", series.to_string()));
    } else {
        let tag = input
            .get("tag_slug")
            .and_then(|v| v.as_str())
            .unwrap_or("weather");
        params.push(("tag_slug", tag.to_string()));
    }

    let events = get_json(&format!("{GAMMA}/events"), &params).await?;
    let list = events.as_array().cloned().unwrap_or_default();
    let summary: Vec<Value> = list
        .iter()
        .map(|e| {
            json!({
                "title": e.get("title"),
                "slug": e.get("slug"),
                "volume_24hr": e.get("volume24hr"),
                "liquidity": e.get("liquidity"),
                "end_date": e.get("endDate"),
                "game_start_time": e.get("gameStartTime"),
                "outcome_count": e.get("markets").and_then(|m| m.as_array()).map(|a| a.len())
            })
        })
        .collect();

    out(json!({
        "count": summary.len(),
        "events": summary,
        "caveat": "The 'weather' tag is a grab bag: alongside daily temperature it carries earthquakes, volcanoes, pandemics and one-off novelty markets. Filter with series_slug (e.g. 'nyc-daily-weather') for the recurring temperature ladders, which are where the volume and the repeatable edge are.",
        "next_step": "Call this tool again with a specific 'slug' to get the verbatim resolution criteria and the ladder token ids."
    }))
}

/// Best bid/ask from a CLOB book. The raw API sorts bids ascending and asks
/// descending, so the best price on each side is the LAST element.
fn best_of(side: Option<&Value>, is_bid: bool) -> Option<(f64, f64)> {
    let arr = side?.as_array()?;
    let mut best: Option<(f64, f64)> = None;
    for lvl in arr {
        let p = lvl
            .get("price")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())?;
        let sz = lvl
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let better = match best {
            None => true,
            Some((bp, _)) => {
                if is_bid {
                    p > bp
                } else {
                    p < bp
                }
            }
        };
        if better {
            best = Some((p, sz));
        }
    }
    best
}

async fn polymarket_orderbook(input: &Value) -> Result<String, String> {
    let token_id = input
        .get("token_id")
        .and_then(|v| v.as_str())
        .ok_or("'token_id' is required (get it from polymarket_weather_markets)")?
        .to_string();

    let book = get_json(&format!("{CLOB}/book"), &[("token_id", token_id.clone())]).await?;
    let bid = best_of(book.get("bids"), true);
    let ask = best_of(book.get("asks"), false);

    let depth = |side: Option<&Value>, is_bid: bool| -> Vec<Value> {
        let mut lvls: Vec<(f64, f64)> = side
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|l| {
                        let p = l.get("price")?.as_str()?.parse::<f64>().ok()?;
                        let s = l.get("size")?.as_str()?.parse::<f64>().ok()?;
                        Some((p, s))
                    })
                    .collect()
            })
            .unwrap_or_default();
        lvls.sort_by(|a, b| {
            if is_bid {
                b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        lvls.into_iter()
            .take(5)
            .map(|(p, s)| json!({ "price": p, "size": s, "notional_usd": r(p * s, 2) }))
            .collect()
    };

    let mid = match (bid, ask) {
        (Some((b, _)), Some((a, _))) => Some((a + b) / 2.0),
        _ => None,
    };
    let spread = match (bid, ask) {
        (Some((b, _)), Some((a, _))) => Some(a - b),
        _ => None,
    };

    // A book can be structurally untradeable rather than merely thin: one
    // side missing, a price pinned at the extreme tick, or an absurd spread.
    // These are the signatures of a market that has effectively already
    // resolved. Valuing them naively produces enormous phantom edges — a
    // resting ask at 0.001 against a 0.55 fair value reads as +54 cents of EV
    // per share, which is not a trade, it is a settled market.
    let mut degenerate: Vec<&str> = Vec::new();
    if bid.is_none() {
        degenerate.push("no bids at all — nobody will buy this outcome from you");
    }
    if ask.is_none() {
        degenerate.push("no asks at all — this outcome cannot be bought");
    }
    if let Some((a, _)) = ask {
        if a <= 0.01 {
            degenerate.push("ask pinned at or below 1 cent — the market treats this outcome as already decided TRUE, or the book is stale");
        }
    }
    if let Some((b, _)) = bid {
        if b >= 0.99 {
            degenerate.push(
                "bid at or above 99 cents — the market treats this outcome as already decided",
            );
        }
    }
    if let Some(s) = spread {
        if s > 0.40 {
            degenerate.push("spread wider than 40 cents — there is no meaningful price here");
        }
    }
    let is_degenerate = !degenerate.is_empty();

    let mut result = json!({
        "token_id": token_id,
        "best_bid": bid.map(|(p, s)| json!({ "price": p, "size": s })),
        "best_ask": ask.map(|(p, s)| json!({ "price": p, "size": s })),
        "midpoint": mid.map(|m| r(m, 4)),
        "spread": spread.map(|s| r(s, 4)),
        "implied_probability": mid.map(|m| r(m, 4)),
        "bids_top5": depth(book.get("bids"), true),
        "asks_top5": depth(book.get("asks"), false),
        "book_quality": {
            "tradeable": !is_degenerate,
            "issues": degenerate.clone()
        },
        "depth_warning": "Top-of-book depth on these markets is typically only a few hundred dollars, often with a much larger wall several ticks away. Size to the book, not to Kelly: walking the book converts your edge into slippage."
    });

    if let Some(q) = input.get("fair_probability").and_then(|v| v.as_f64()) {
        if !(0.0..=1.0).contains(&q) {
            return Err("fair_probability must be in [0, 1]".into());
        }
        let bankroll = input
            .get("bankroll_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(1000.0);
        let kf = input
            .get("kelly_fraction")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.25)
            .clamp(0.0, 1.0);

        // Buying YES: pay the ask. Buying NO is equivalent to selling YES at
        // the bid, so its effective cost is 1 - bid.
        let mut sides = Vec::new();
        for (name, entry_price, win_prob) in [
            ("buy_yes", ask.map(|(a, _)| a), q),
            ("buy_no", bid.map(|(b, _)| 1.0 - b), 1.0 - q),
        ] {
            let Some(p) = entry_price else { continue };
            if p <= 0.0 || p >= 1.0 {
                continue;
            }
            // Polymarket taker fee, per share.
            let fee_taker = 0.05 * p * (1.0 - p);
            let fee_maker = fee_taker * 0.75; // 25% maker rebate
            let gross_ev = win_prob - p;
            let ev_taker = gross_ev - fee_taker;
            let ev_maker = gross_ev - fee_maker;
            // Kelly for a binary contract costing p that pays 1.
            let kelly_full = if p < 1.0 {
                (win_prob - p) / (1.0 - p)
            } else {
                0.0
            };
            let kelly_used = (kelly_full * kf).max(0.0);

            sides.push(json!({
                "side": name,
                "entry_price": r(p, 4),
                "your_win_probability": r(win_prob, 4),
                "gross_edge_per_share": r(gross_ev, 4),
                "fee_per_share_taker": r(fee_taker, 5),
                "fee_per_share_maker": r(fee_maker, 5),
                "ev_per_share_taker": r(ev_taker, 4),
                "ev_per_share_maker": r(ev_maker, 4),
                "ev_pct_of_notional_taker": r(ev_taker / p * 100.0, 2),
                "breakeven_probability_taker": r(p + fee_taker, 4),
                "kelly_full": r(kelly_full, 4),
                "kelly_fractional": r(kelly_used, 4),
                "recommended_stake_usd": if is_degenerate { json!(0.0) } else { r(kelly_used * bankroll, 2) },
                "verdict": if is_degenerate {
                    "DO NOT TRADE — the book is degenerate (see book_quality.issues). Any apparent edge here is an artefact of a one-sided or already-resolved market, not a mispricing."
                } else if ev_taker > 0.02 {
                    "clear edge as taker"
                } else if ev_maker > 0.0 {
                    "edge only survives as MAKER — post a resting order, do not cross the spread"
                } else {
                    "no edge after fees — pass"
                }
            }));
        }

        result["valuation"] = json!({
            "your_fair_probability": q,
            "bankroll_usd": bankroll,
            "kelly_fraction_applied": kf,
            "sides": sides,
            "fee_model": "Polymarket taker fee = 0.05 * p * (1-p) per share, maximised at 1.25 cents per share at p=0.5, i.e. 2.5% of notional. Maker orders receive a 25% rebate. This fee is large enough to erase most apparent weather edges, which is why both cases are reported.",
            "discipline": "A Kelly stake is only correct if your probability is CALIBRATED. On an uncalibrated weather probability full Kelly is a fast route to ruin; the default 0.25 fraction is deliberate. Do not raise it until a live reliability diagram shows your stated probabilities verifying at their claimed frequency."
        });
    }

    out(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stations_are_sorted_and_unique() {
        for w in STATIONS.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "stations must be sorted+unique: {} {}",
                w[0].0,
                w[1].0
            );
        }
    }

    #[test]
    fn every_series_maps_to_a_known_station() {
        for e in SERIES_MAP {
            assert!(
                station_by_icao(e.1).is_some(),
                "series '{}' points at unknown station '{}'",
                e.0,
                e.1
            );
        }
    }

    #[test]
    fn station_coordinates_are_plausible() {
        for s in STATIONS {
            assert!((-90.0..=90.0).contains(&s.4), "{} bad lat", s.0);
            assert!((-180.0..=180.0).contains(&s.5), "{} bad lon", s.0);
            assert_eq!(s.3.len(), 2, "{} bad iso2", s.0);
            assert!(s.7.contains('/'), "{} tz should be IANA", s.0);
        }
    }

    #[test]
    fn resolves_the_station_traps() {
        // The whole point of the registry: these are the ones a naive
        // implementation gets wrong.
        assert_eq!(resolve_series("NYC").unwrap().1, "KLGA");
        assert_eq!(resolve_series("New York").unwrap().1, "KLGA");
        assert_eq!(resolve_series("dallas-daily-weather").unwrap().1, "KDAL");
        assert_eq!(resolve_series("denver").unwrap().1, "KBKF");
        assert_eq!(resolve_series("paris").unwrap().1, "LFPB");
        assert_eq!(resolve_series("london").unwrap().1, "EGLC");
        assert_eq!(resolve_series("seoul").unwrap().1, "RKSI");
        assert_eq!(resolve_series("taipei").unwrap().1, "RCSS");
    }

    #[test]
    fn longest_series_stem_wins() {
        // "kuala-lumpur" must not be shadowed, and a full event slug must
        // resolve as well as a bare city name.
        assert_eq!(
            resolve_series("kuala-lumpur-daily-weather").unwrap().1,
            "WMKK"
        );
        assert_eq!(
            resolve_series("highest-temperature-in-nyc-on-august-14-2026")
                .unwrap()
                .1,
            "KLGA"
        );
        assert_eq!(resolve_series("san-francisco").unwrap().1, "KSFO");
    }

    #[test]
    fn us_markets_are_fahrenheit_and_others_celsius() {
        assert_eq!(resolve_series("nyc").unwrap().2, "fahrenheit");
        assert_eq!(resolve_series("nyc").unwrap().3, 2.0);
        assert_eq!(resolve_series("tokyo").unwrap().2, "celsius");
        assert_eq!(resolve_series("tokyo").unwrap().3, 1.0);
        // Hong Kong is the only 0.1-degree market.
        assert_eq!(resolve_series("hong-kong").unwrap().3, 0.1);
    }

    #[test]
    fn model_aliases_resolve_open_meteo_renames() {
        // Requesting `gfs025` yields response keys under `ncep_gefs025`. If we
        // do not recognise that, a working model is reported as missing.
        assert!(model_names_match("gfs025", "ncep_gefs025"));
        assert!(model_names_match("ecmwf_ifs025", "ecmwf_ifs025"));
        assert!(model_names_match(
            "bom_access_global_ensemble",
            "bom_access_global_ensemble"
        ));
        // Genuinely different models must NOT collapse together.
        assert!(!model_names_match("icon_eu", "icon_global"));
        assert!(!model_names_match("gfs025", "gem_global"));
    }

    #[test]
    fn model_attribution_handles_control_and_members() {
        assert_eq!(
            model_of_key(
                "temperature_2m_max_member01_ecmwf_ifs025_ensemble",
                "temperature_2m_max"
            ),
            Some("ecmwf_ifs025".to_string())
        );
        // The control run has no member index.
        assert_eq!(
            model_of_key(
                "temperature_2m_max_ecmwf_ifs025_ensemble",
                "temperature_2m_max"
            ),
            Some("ecmwf_ifs025".to_string())
        );
        assert_eq!(
            model_of_key(
                "temperature_2m_max_member40_icon_eu_ensemble",
                "temperature_2m_max"
            ),
            Some("icon_eu".to_string())
        );
        assert_eq!(
            model_of_key(
                "precipitation_sum_member18_bom_access_global_ensemble_ensemble",
                "precipitation_sum"
            ),
            Some("bom_access_global_ensemble".to_string())
        );
    }

    /// A verbatim excerpt of the real KLGA CLI product issued 2026-08-13
    /// 06:17Z, which is the FINAL summary for 2026-08-12 (the preliminary,
    /// issued 20:34Z the day before, reported MAXIMUM 86).
    const REAL_CLI: &str = "\
CLIMATE REPORT\n\
NATIONAL WEATHER SERVICE NEW YORK, NY\n\
217 AM EDT THU AUG 13 2026\n\
\n\
...THE LAGUARDIA NY CLIMATE SUMMARY FOR AUGUST 12 2026...\n\
\n\
CLIMATE NORMAL PERIOD 1991 TO 2020\n\
\n\
WEATHER ITEM   OBSERVED TIME   RECORD YEAR NORMAL DEPARTURE LAST\n\
                VALUE   (LST)  VALUE       VALUE  FROM      YEAR\n\
TEMPERATURE (F)\n\
 YESTERDAY\n\
  MAXIMUM         87    434 PM  98    2016  85      2       91\n\
  MINIMUM         74    650 AM  56    1979  72      2       72\n\
  AVERAGE         81                        78      3       82\n\
";

    #[test]
    fn cli_row_parses_the_columnar_layout() {
        let max = cli_row(REAL_CLI, "MAXIMUM").expect("MAXIMUM row");
        // The observed value is what settles the market.
        assert_eq!(max["observed"], 87.0);
        assert_eq!(max["observed_at_local"], "434 PM");
        // Everything to the right of the time must NOT be mistaken for it.
        assert_eq!(max["record"], 98.0);
        assert_eq!(max["record_year"], 2016);
        assert_eq!(max["normal_1991_2020"], 85.0);
        assert_eq!(max["departure_from_normal"], 2.0);
        assert_eq!(max["same_day_last_year"], 91.0);

        let min = cli_row(REAL_CLI, "MINIMUM").expect("MINIMUM row");
        assert_eq!(min["observed"], 74.0);
        assert_eq!(min["record"], 56.0);
        assert_eq!(min["normal_1991_2020"], 72.0);

        assert!(cli_row(REAL_CLI, "SNOW DEPTH").is_none());
    }

    #[test]
    fn cli_summary_date_is_yesterday_not_the_issuance_day() {
        // The product is issued on AUG 13 but summarises AUG 12. An agent that
        // conflates the two prices the wrong day's market.
        let d = cli_summary_date(REAL_CLI).expect("summary date");
        assert_eq!(d, "AUGUST 12 2026");
        assert!(!d.contains("13"));
    }

    #[test]
    fn quantiles_interpolate() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(quantile(&xs, 0.0), 1.0);
        assert_eq!(quantile(&xs, 0.5), 3.0);
        assert_eq!(quantile(&xs, 1.0), 5.0);
        assert!((quantile(&xs, 0.25) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn ols_recovers_a_known_slope() {
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 + 2.0 * x).collect();
        let (slope, intercept) = ols(&xs, &ys).unwrap();
        assert!((slope - 2.0).abs() < 1e-9);
        assert!((intercept - 3.0).abs() < 1e-9);
    }

    #[test]
    fn precipitation_switches_the_nyc_station() {
        let spec = settlement_spec(&json!({ "city": "nyc", "variable": "precipitation" })).unwrap();
        let v: Value = serde_json::from_str(&spec).unwrap();
        assert_eq!(v["settlement_station"]["icao"], "KNYC");

        let temp = settlement_spec(&json!({ "city": "nyc", "variable": "high_temp" })).unwrap();
        let t: Value = serde_json::from_str(&temp).unwrap();
        assert_eq!(t["settlement_station"]["icao"], "KLGA");
    }

    #[test]
    fn non_us_stations_are_flagged_as_unverifiable() {
        let spec = settlement_spec(&json!({ "city": "tokyo" })).unwrap();
        let v: Value = serde_json::from_str(&spec).unwrap();
        let warnings = v["warnings"].as_array().unwrap();
        assert!(warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("NON-US STATION")));
    }

    #[test]
    fn best_of_finds_the_best_price_despite_api_ordering() {
        // The CLOB API returns bids ascending and asks descending, so the best
        // price is the LAST element on each side, not the first.
        let bids = json!([
            { "price": "0.40", "size": "20084" },
            { "price": "0.42", "size": "408" },
            { "price": "0.43", "size": "519" }
        ]);
        let asks = json!([
            { "price": "0.46", "size": "458" },
            { "price": "0.45", "size": "215" },
            { "price": "0.44", "size": "181" }
        ]);
        assert_eq!(best_of(Some(&bids), true), Some((0.43, 519.0)));
        assert_eq!(best_of(Some(&asks), false), Some((0.44, 181.0)));
        assert_eq!(best_of(None, true), None);
    }

    // ── Portfolio risk ──────────────────────────────────────────────────

    #[test]
    fn ladder_kelly_matches_the_classic_horse_race_threshold() {
        // Classic result: include outcome i iff q_i/p_i exceeds
        //   (1 - sum_S q) / (1 - sum_S p)
        // over the included set S. Negative-edge legs DO qualify as hedges
        // when mutually exclusive, which is counterintuitive and worth pinning.
        let q = vec![
            0.0002, 0.0003, 0.0006, 0.0071, 0.0006, 0.0157, 0.1340, 0.3761, 0.3521, 0.1099, 0.0116,
        ];
        let p = vec![
            0.0015, 0.0010, 0.0015, 0.0090, 0.0650, 0.1450, 0.3350, 0.2950, 0.1450, 0.0450, 0.0170,
        ];
        let f = ladder_kelly(&q, &p);

        // Bucket 27/28/29 carry the positive edge and must be funded.
        assert!(f[7] > 0.2, "bucket 27 underfunded: {}", f[7]);
        assert!(f[8] > 0.2, "bucket 28 underfunded: {}", f[8]);
        assert!(f[9] > 0.05, "bucket 29 underfunded: {}", f[9]);

        // Bucket 26 has edge -0.20 but q/p = 0.40, above the ~0.315 threshold,
        // so it is a legitimate hedge and must be funded despite negative EV.
        assert!(
            f[6] > 0.01,
            "bucket 26 has negative edge but q/p above threshold — it is a hedge, got {}",
            f[6]
        );

        // Buckets 24 and 25 have q/p of 0.009 and 0.108, far below threshold.
        assert!(f[4] < 1e-3, "bucket 24 should be excluded, got {}", f[4]);
        assert!(f[5] < 1e-3, "bucket 25 should be excluded, got {}", f[5]);

        // Feasible: never stake more than the bankroll.
        let total: f64 = f.iter().sum();
        assert!(total > 0.0 && total < 1.0, "total stake {total} infeasible");
        // And it must beat the naive per-bucket allocation on log growth.
        let naive: Vec<f64> = q
            .iter()
            .zip(&p)
            .map(|(qi, pi)| ((qi - pi) / (1.0 - pi)).max(0.0))
            .collect();
        assert!(
            log_growth(&f, &q, &p) > log_growth(&naive, &q, &p),
            "multi-outcome Kelly must dominate summed per-bucket Kelly"
        );
    }

    #[tokio::test]
    async fn an_incomplete_ladder_is_rejected_not_levered_into() {
        // Feeding only the central buckets of an 11-bucket ladder makes the
        // omitted mass look like free money: prices summed to 0.965 against
        // model probabilities of 0.988, and the optimiser staked 95% of
        // bankroll on a phantom arbitrage.
        let r = dispatch(
            "weather_portfolio_risk",
            &json!({ "ladder": [
                { "label": "27", "model_prob": 0.376, "price": 0.295 },
                { "label": "28", "model_prob": 0.352, "price": 0.145 }
            ]}),
        )
        .await
        .expect("dispatched");
        let err = r.expect_err("an incomplete ladder must be rejected");
        assert!(err.contains("INCOMPLETE"), "unhelpful error: {err}");
    }

    #[tokio::test]
    async fn a_ladder_read_as_thresholds_is_rejected() {
        // Reading "32C" as ">= 32C" for every bucket makes the probabilities
        // sum far above 1 — the 6x error class this whole stack guards against.
        let r = dispatch(
            "weather_portfolio_risk",
            &json!({ "ladder": [
                { "label": "26", "model_prob": 0.95, "price": 0.335 },
                { "label": "27", "model_prob": 0.80, "price": 0.295 },
                { "label": "28", "model_prob": 0.45, "price": 0.145 }
            ]}),
        )
        .await
        .expect("dispatched");
        let err = r.expect_err("probabilities summing above 1 must be rejected");
        assert!(
            err.contains("mutually exclusive"),
            "error should name the cause: {err}"
        );
    }

    #[test]
    fn correlation_is_symmetric_and_bounded() {
        let a = vec![1.0, -2.0, 3.0, 0.5, -1.5, 2.2, 0.1];
        let b = vec![0.9, -1.8, 2.7, 0.4, -1.2, 2.0, 0.3];
        let c = corr(&a, &b);
        assert!((-1.0..=1.0).contains(&c), "correlation {c} out of range");
        assert!(
            (c - corr(&b, &a)).abs() < 1e-12,
            "correlation must be symmetric"
        );
        assert!(
            (corr(&a, &a) - 1.0).abs() < 1e-9,
            "self-correlation must be 1"
        );
        // Too few points to be meaningful.
        assert!(corr(&[1.0, 2.0], &[1.0, 2.0]).is_nan());
    }

    #[test]
    fn tool_names_are_unique() {
        let defs = tool_defs();
        let mut names: Vec<&str> = defs.iter().map(|d| d.name).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(
            before,
            names.len(),
            "duplicate tool name in weather tool_defs"
        );
    }

    #[tokio::test]
    async fn dispatch_covers_every_declared_tool() {
        // A declared tool with no dispatch arm is a phantom tool: it gets
        // advertised to the model, called, and fails with "Unknown tool".
        for def in tool_defs() {
            let r = dispatch(def.name, &json!({})).await;
            assert!(
                r.is_some(),
                "tool '{}' is declared but not dispatched",
                def.name
            );
        }
        assert!(dispatch("definitely_not_a_weather_tool", &json!({}))
            .await
            .is_none());
    }

    #[test]
    fn weather_tools_are_registered_as_platform_tools() {
        // The weather defs must actually reach `platform_tools()`, otherwise
        // agent cards declaring them fail publication validation even though
        // the dispatch arms exist.
        let platform = crate::agent_backend::tools::platform_tool_names();
        for def in tool_defs() {
            assert!(
                platform.contains(&def.name),
                "'{}' is defined here but never reaches platform_tools(); the card validator \
                 would reject it as a phantom tool",
                def.name
            );
        }
    }

    /// Every tool the weather agent cards declare must be dispatchable.
    ///
    /// This is the test that would have caught the historical phantom-tool
    /// class of bug: a card advertising a capability the runtime cannot
    /// service, which reaches the model and fails at call time with
    /// `Unknown tool: X`.
    #[test]
    fn weather_agent_cards_declare_no_phantom_tools() {
        use std::path::Path;

        let dir = [
            Path::new("agents/curated"),
            Path::new("../../agents/curated"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("run from the workspace root");

        let platform = crate::agent_backend::tools::platform_tool_names();
        let agents = [
            "weather_oracle",
            "weather_ensemble_forecaster",
            "weather_calibrator",
            "weather_market_analyst",
        ];

        let mut checked = 0;
        for agent in agents {
            let path = dir.join(agent).join("agent_card.json");
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let card: Value = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));

            let tools = card["capabilities"]["mcp_tools"]
                .as_array()
                .unwrap_or_else(|| panic!("{agent} has no mcp_tools array"));
            assert!(!tools.is_empty(), "{agent} declares no tools");

            for t in tools {
                let name = t["name"].as_str().expect("tool name must be a string");
                assert!(
                    platform.contains(&name),
                    "{agent} declares '{name}', which is not a dispatchable platform tool"
                );
                // NO `input_schema` requirement, deliberately.
                //
                // It used to be asserted here, and adding a tool to a card
                // failed on it. The requirement is dead in two independent
                // ways:
                //
                //   * `ToolRegistry::to_claude_tools_with_card_and_remote`
                //     uses a card's `input_schema` ONLY for a tool the
                //     registry does not already have. The assertion above
                //     proves every tool here IS a platform tool, so the card's
                //     copy is never read.
                //   * measured when this was relaxed: **19 of 19** schemas on
                //     the four weather cards differ from the registry's. Not
                //     one matched. They are stale copies of a thing the
                //     registry owns.
                //
                // So requiring one made adding a true declaration cost a
                // twentieth divergent copy. If these should be checked rather
                // than dropped, the assertion to write is equality with
                // `all_tools()`, not presence — and that is a bigger change
                // than this test. Recorded in
                // docs/ISSUES_tool_declaration_gap.md.
                checked += 1;
            }
        }
        assert!(
            checked >= 14,
            "expected to check more tool declarations, saw {checked}"
        );
    }

    /// No curated card anywhere may declare a tool the runtime cannot dispatch.
    ///
    /// The sibling test above covers four weather agents by name, which is how
    /// the class survived everywhere else: `moe_router_strategist`,
    /// `debate_strategist` and `vote_strategist` all declared
    /// `get_agent_calibration`, and `cohere_and_coordinate` declared
    /// `propose_composition_change`, with no dispatch arm behind either. Both
    /// broke a feedback loop — Loop 5's router could not read calibration, and
    /// Loop 4 could not receive a proposal — and both failed as a runtime
    /// string, `Unknown tool: X`, which reads to an operator as the model
    /// misbehaving rather than the platform lying about its capabilities.
    ///
    /// `invalid_tool_declarations` has existed for a while and catches exactly
    /// this, but only runs on the DB agent-update path, so filesystem cards
    /// were never checked. This closes that.
    ///
    /// If this fails: either add the dispatch arm, or remove the declaration.
    /// Do not add the name to an allowlist — a declared tool that cannot run is
    /// worse than an absent one, because the model will confidently call it.
    #[test]
    fn no_curated_card_declares_a_phantom_tool() {
        use std::path::Path;

        let dir = [
            Path::new("agents/curated"),
            Path::new("../../agents/curated"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("run from the workspace root");

        // Two different questions, and conflating them misreports working
        // tools as broken:
        //
        //   * `dispatchable` — is there a match arm in `ToolRegistry::execute`?
        //     If not, the model is advertised the tool (card tools carrying a
        //     schema are passed through verbatim), calls it, and receives
        //     `Unknown tool: X`. That is real breakage.
        //   * `declarable` — is it in `builtin_tools()`? `invalid_tool_declarations`
        //     gates card writes on this, so a tool with an arm but no
        //     `BuiltinToolDef` works perfectly at run time yet cannot be saved
        //     through the API.
        //
        // `equity_analyst`'s nine `fmp_*` tools are exactly the second case:
        // fully implemented at `tools_legacy.rs` `execute_fmp_api`, never
        // registered as defs. An earlier version of this test called them
        // phantom, which is the same imprecision it exists to prevent.
        let declarable = crate::agent_backend::tools::platform_tool_names();
        let dispatchable = crate::agent_backend::tools::dispatchable_tool_names();
        let mut offenders: Vec<String> = Vec::new();
        let mut undeclarable: Vec<String> = Vec::new();
        let mut cards_checked = 0usize;

        for entry in std::fs::read_dir(dir).expect("cannot read agents/curated") {
            let entry = entry.expect("bad dir entry");
            let card_path = entry.path().join("agent_card.json");
            if !card_path.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(&card_path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", card_path.display()));
            let card: Value = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", card_path.display()));
            cards_checked += 1;

            let agent = card["agent_id"].as_str().unwrap_or("<unnamed>").to_string();

            // A card may legitimately reach tools on a remote MCP server it
            // declares; those are resolved at run time and cannot be checked
            // from disk. Only cards with no `mcp_servers` must resolve every
            // declared tool against the platform set.
            let has_remote = card["capabilities"]["mcp_servers"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            if has_remote {
                continue;
            }

            let Some(tools) = card["capabilities"]["mcp_tools"].as_array() else {
                continue;
            };
            for t in tools {
                let Some(name) = t["name"].as_str() else {
                    continue;
                };
                if !dispatchable.contains(&name) {
                    // Will fail at call time.
                    offenders.push(format!("{agent} → {name}"));
                } else if !declarable.contains(&name) {
                    // Runs, but the card cannot be re-saved.
                    undeclarable.push(format!("{agent} → {name}"));
                }
            }
        }

        assert!(
            cards_checked > 50,
            "only scanned {cards_checked} cards — the glob is probably wrong"
        );

        // Ratchet, not a clean sheet.
        //
        // 92 declarations across the curated corpus were already phantom when
        // this test was written. Fixing them all is a separate piece of work
        // (most are third-party integrations — Bluesky, adaptogen, AR — that
        // need real dispatch arms or removal, decided case by case). Asserting
        // `offenders.is_empty()` today would mean deleting the test tomorrow,
        // which is how the class survived in the first place.
        //
        // So: anything NOT on this list is a hard failure, and the list may
        // only shrink. Fix a card, remove its line. Never add one.
        let known_debt: &[&str] = &[
            "adaptogen_curator → adaptogen_compare_species",
            "adaptogen_curator → adaptogen_compound_search",
            "adaptogen_curator → adaptogen_drug_interaction_check",
            "adaptogen_curator → adaptogen_evidence_query",
            "adaptogen_curator → adaptogen_genomic_markers",
            "adaptogen_curator → adaptogen_indication_search",
            "adaptogen_curator → adaptogen_medicine_system_browse",
            "adaptogen_curator → adaptogen_population_variants",
            "adaptogen_curator → adaptogen_safety_check",
            "adaptogen_curator → adaptogen_species_detail",
            "adaptogen_curator → adaptogen_species_search",
            "adaptogen_curator → adaptogen_traditional_use_query",
            "ar_avatar_renderer → avatar_profile_loader",
            "ar_avatar_renderer → interaction_designer",
            "ar_avatar_renderer → scene_planner",
            "bioreactor_modeler → get_latest_observation",
            "bioreactor_modeler → list_active_sessions",
            "bioreactor_modeler → send_actuation",
            "biotech_analyst → get_ontology_analytics",
            "biotech_analyst → search_ontology_properties",
            "biotech_analyst → search_ontology_terms",
            "bluesky_publisher → create_post",
            "bluesky_publisher → create_thread",
            "bluesky_publisher → fetch_og_metadata",
            "bluesky_publisher → get_post",
            "bluesky_publisher → resolve_handle",
            "bluesky_publisher → upload_blob",
            "coherence_consultant → ontology_reader",
            "companion_builder_coach → agent_template_loader",
            "companion_builder_coach → design_checklist",
            "daily_puzzle → puzzle_generator",
            "daily_puzzle → streak_tracker",
            "dream_coordinator → consolidation_reader",
            "dream_narrator → agent_profile_loader",
            "dream_narrator → consolidation_reader",
            "dyad_observer → query_episodes",
            "dyad_observer → query_persona_history",
            "embedding_projector_guide → cluster_interpreter",
            "embedding_projector_guide → projection_api",
            "embedding_projector_guide → temporal_analysis",
            "instagram_publisher → check_container_status",
            "instagram_publisher → create_media_container",
            "instagram_publisher → get_account_info",
            "instagram_publisher → get_media_insights",
            "instagram_publisher → list_recent_media",
            "instagram_publisher → publish_media",
            "micro_patron_template → agent_card_generator",
            "micro_patron_template → pricing_calculator",
            "performance_coach → agent_stats_api",
            "performance_coach → benchmark_comparator",
            "performance_coach → ontology_analyzer",
            "pipeline_strategist → get_workflow_template",
            "simops_companion → annotate",
            "simops_companion → annotate_schema",
            "simops_companion → compare",
            "simops_companion → fork_state",
            "simops_companion → invoke_member",
            "simops_companion → mutate_document",
            "social_media_studio → create_bsky_post",
            "social_media_studio → create_media_container",
            "social_media_studio → get_media_insights",
            "social_media_studio → publish_media",
            "social_media_studio → upload_bsky_blob",
            "stripe_billing → charge_usage",
            "stripe_billing → check_connect_status",
            "stripe_billing → create_checkout_session",
            "stripe_billing → create_connect_account",
            "stripe_billing → generate_client_api_key",
            "stripe_billing → get_payout_balance",
            "stripe_billing → get_usage_summary",
            "stripe_billing → record_usage",
            "stripe_billing → set_pricing",
            "wild_companion → log_observation",
        ];

        let new_offenders: Vec<&String> = offenders
            .iter()
            .filter(|o| !known_debt.contains(&o.as_str()))
            .collect();
        assert!(
            new_offenders.is_empty(),
            "{} card(s) declare a tool with no dispatch arm. Add the arm and a \
             BuiltinToolDef, or remove the declaration — do not add to known_debt:\n  {}",
            new_offenders.len(),
            new_offenders
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        );

        // Separate assertion, separate failure message: a tool that runs but
        // cannot be declared is a real defect, and a different one.
        assert!(
            undeclarable.is_empty(),
            "{} declaration(s) have a dispatch arm but no BuiltinToolDef. They work \
             at run time, but `invalid_tool_declarations` will reject the card on \
             any write \u{2014} register them in `builtin_tools_core()`:\n  {}",
            undeclarable.len(),
            undeclarable.join("\n  ")
        );

        // The list may only shrink: a fixed card must be removed from it, or
        // the ratchet silently stops protecting that card.
        let stale: Vec<&&str> = known_debt
            .iter()
            .filter(|d| !offenders.iter().any(|o| o == *d))
            .collect();
        assert!(
            stale.is_empty(),
            "{} known_debt entr(ies) are now dispatchable — delete them from the \
             list so it keeps ratcheting down:\n  {:?}",
            stale.len(),
            stale
        );
    }

    /// Loop 3's cascade tool must be declared by the strategist and must be
    /// dispatchable, or coordination findings never reach member memory.
    ///
    /// The failure this guards is subtle: the card's Stage 4 previously told
    /// the agent to "write a context episode via `write_workspace_file` to
    /// `_coordination/cascade/<agent>.md`", which reads like the right thing
    /// and does nothing. Consolidation reads `episodes`; nothing reads that
    /// path. The loop appeared to run and taught no one anything.
    #[test]
    fn strategist_can_write_into_member_memory() {
        use std::path::Path;

        let dir = [
            Path::new("agents/curated"),
            Path::new("../../agents/curated"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("run from the workspace root");

        let raw =
            std::fs::read_to_string(dir.join("cohere_and_coordinate/agent_card.json")).unwrap();
        let card: Value = serde_json::from_str(&raw).unwrap();

        let tools = card["capabilities"]["mcp_tools"].as_array().unwrap();
        let declared: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(
            declared.contains(&"record_coordination_observation"),
            "cohere_and_coordinate must declare record_coordination_observation — \
             without it Loop 3 has no path into member memory"
        );

        let platform = crate::agent_backend::tools::platform_tool_names();
        assert!(
            platform.contains(&"record_coordination_observation"),
            "record_coordination_observation must be a dispatchable platform tool"
        );

        // Stage 4 must instruct the memory write, not the file write. A card
        // that still says `_coordination/cascade/` is describing a mechanism
        // that cannot reach dreaming.
        let prompt = card["system_prompt"].as_str().unwrap();
        assert!(
            prompt.contains("record_coordination_observation"),
            "the Stage 4 cascade must call record_coordination_observation"
        );
        assert!(
            !prompt.contains("_coordination/cascade/"),
            "Stage 4 still describes the file-based cascade, which dreaming cannot read"
        );
    }

    /// The `weather_oracle` card is the orchestra contract. Its shape is load
    /// bearing: `validate_fermi_contract` in handlers/orchestras.rs requires a
    /// non-empty `finding_labels` and a well-ordered `multiplier_range`.
    #[test]
    fn weather_oracle_fermi_contract_is_valid() {
        use std::path::Path;

        let dir = [
            Path::new("agents/curated"),
            Path::new("../../agents/curated"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("run from the workspace root");
        let raw = std::fs::read_to_string(dir.join("weather_oracle/agent_card.json")).unwrap();
        let card: Value = serde_json::from_str(&raw).unwrap();

        let fc = &card["capabilities"]["fermi_contract"];
        assert!(
            fc.is_object(),
            "weather_oracle must declare a fermi_contract"
        );

        let labels = fc["finding_labels"]
            .as_array()
            .expect("finding_labels array");
        assert!(!labels.is_empty());
        // The orchestra protocol expects a MULTIPLIER terminator.
        assert!(
            labels.iter().any(|l| l.as_str() == Some("MULTIPLIER")),
            "finding_labels must include MULTIPLIER per the Fermi orchestra protocol"
        );

        let range = fc["multiplier_range"].as_array().expect("multiplier_range");
        assert_eq!(range.len(), 2);
        let lo = range[0].as_f64().unwrap();
        let hi = range[1].as_f64().unwrap();
        assert!(lo < hi, "multiplier_range min must be < max");

        // Seed facts populate the CEP knowledge graph on first run, so they
        // must carry the full shape the loader expects.
        let seeds = fc["seed_facts"].as_array().expect("seed_facts array");
        assert!(seeds.len() >= 5, "expected a substantive seed-fact set");
        for (i, s) in seeds.iter().enumerate() {
            for field in ["entity_type", "name", "description"] {
                assert!(
                    s[field].as_str().is_some_and(|v| !v.is_empty()),
                    "seed_facts[{i}] missing '{field}'"
                );
            }
            assert!(s["properties"].is_object(), "seed_facts[{i}] properties");
            let c = s["confidence"].as_f64().unwrap_or(-1.0);
            assert!(
                (0.0..=1.0).contains(&c),
                "seed_facts[{i}] confidence {c} out of range"
            );
        }
    }

    /// The composition must be internally consistent: every agent named in
    /// `weather_oracle`'s dependencies and workflow stages must exist on disk.
    #[test]
    fn weather_oracle_composition_members_all_exist() {
        use std::path::Path;

        let dir = [
            Path::new("agents/curated"),
            Path::new("../../agents/curated"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("run from the workspace root");
        let raw = std::fs::read_to_string(dir.join("weather_oracle/agent_card.json")).unwrap();
        let card: Value = serde_json::from_str(&raw).unwrap();

        let mut referenced: Vec<String> = Vec::new();
        for key in ["required", "optional"] {
            for a in card["dependencies"][key].as_array().unwrap_or(&vec![]) {
                referenced.push(a.as_str().unwrap().to_string());
            }
        }
        let stages = card["workflow_template"]["stages"]
            .as_array()
            .expect("workflow_template.stages");
        assert!(stages.len() >= 4, "pipeline should have at least 4 stages");
        for s in stages {
            referenced.push(s["agent"].as_str().expect("stage agent").to_string());
        }

        for agent in &referenced {
            assert!(
                dir.join(agent).join("agent_card.json").exists(),
                "weather_oracle references '{agent}' but no such agent card exists"
            );
        }

        // The two required members must be the forecast and calibrate stages:
        // pricing is optional (you can forecast without a market), but you can
        // never price without calibrating first.
        let required: Vec<&str> = card["dependencies"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"weather_ensemble_forecaster"));
        assert!(required.contains(&"weather_calibrator"));
    }
}
