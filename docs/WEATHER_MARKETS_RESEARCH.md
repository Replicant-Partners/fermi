# Weather Prediction Markets & Weather Data APIs — Research Brief

**Compiled:** 2026-08-13. All statements marked ✅ were verified by live HTTP request at that
date. Statements marked ⚠️ **unverified** could not be confirmed and should not be relied on.

---

# PART A — Polymarket Weather Markets

## A.1 Scope of the category (✅ verified)

`https://polymarket.com/weather` currently lists **230 markets** across subcategories with these
live counts (from the page's own left-nav):

| Subcategory | Count |
|---|---|
| Temperature — High Temp | 139 |
| Temperature — Low Temp | 22 |
| Precipitation | 5 |
| Global | 29 |
| Tornadoes | 3 |
| Hurricanes | 13 |
| Earthquakes | 15 |
| Volcanoes | 2 |
| Pandemics | 13 |

Note the category is a *grab bag*: "Weather" (Gamma `tag_id=84`, `tag_slug=weather`) also carries
earthquakes, volcanoes, pandemics, meteor strikes and even a Blue Origin launch market. Filter by
`series` slug, not by tag, if you only want meteorology.

## A.2 Archetype 1 — Daily city high/low temperature (the volume core)

### Exact question wording

Event title (the container):

> `Highest temperature in NYC on August 14?`
> `Lowest temperature in NYC on August 13?`
> `Highest temperature in Seoul (Incheon) on August 14?`

Each event is a **negRisk group of ~11 mutually exclusive binary markets**. The individual market
questions are (✅ verified verbatim from Gamma):

> `Will the highest temperature in New York City be 81°F or below on August 14?`
> `Will the highest temperature in New York City be between 86-87°F on August 14?`
> `Will the highest temperature in New York City be 100°F or higher on August 14?`
> `Will the highest temperature in London be 29°C or below on August 14?` *(metric cities are 1° buckets)*

### Bucket structure (✅ verified)

- **Fahrenheit cities (US only):** 11 outcomes, **2°F-wide** buckets, open-ended at both ends.
  e.g. NYC 2026-08-14: `["81°F or below","82-83","84-85","86-87","88-89","90-91","92-93","94-95","96-97","98-99","100°F or higher"]`
- **Celsius cities (everywhere else):** 11 outcomes, **1°C-wide** buckets (single integers), open-ended ends.
  e.g. London 2026-08-14: `["29°C or below","30","31","32","33","34","35","36","37","38","39°C or higher"]`
  e.g. Tokyo 2026-08-14: `["22°C or below","23",…,"31","32°C or higher"]`
- Bucket ladders are **re-centred each day** around the forecast. Some low-liquidity events use a
  truncated ladder (e.g. Jinan showed `"26°C or higher"` / `"24°C"`).

### Resolution source — VERBATIM rules text

There are **three distinct rule variants**. This is the single most important thing to get right.

**Variant 1 — Weather Underground (the majority: ~44 of 51 daily series):**

> This market will resolve based on the highest temperature recorded in the 'Daily Observations'
> table on Weather Underground, not the figure displayed in the 'Day High & Low' summary section;
> in the event of any discrepancy between the two, the Daily Observations table shall be the
> primary resolution source not the Day High & Low section.
>
> This market will resolve to the temperature range that contains the highest temperature recorded
> at the LaGuardia Airport Station in degrees Fahrenheit on 14 Aug '26.
>
> The resolution source for this market will be information from Wunderground, specifically the
> highest temperature recorded for all times on this day for the LaGuardia Airport Station,
> available here: https://www.wunderground.com/history/daily/us/ny/new-york-city/KLGA.
>
> To toggle between Fahrenheit and Celsius, click the gear icon next to the search bar and switch
> the Temperature setting between °F and °C.
>
> This market can not resolve until the first data point for the following date has been published
> on the resolution source.
>
> The resolution source for this market measures temperatures to whole degrees Fahrenheit (eg,
> 21°F). Thus, this is the level of precision that will be used when resolving the market.
>
> Revisions to temperatures recorded within this market's timeframe will be considered until the
> first datapoint for the following date has been published, after which any alterations will not
> be considered.

**Variant 2 — NOAA / weather.gov WRH time-series (Tel Aviv, Moscow, Istanbul):**

> This market will resolve to the temperature range that contains the highest temperature recorded
> by NOAA at the Ben Gurion International Airport in degrees Celsius on 14 Aug '26.
>
> The resolution source for this market will be information from NOAA, specifically the highest
> reading under the "Temp" column for all times on this day, available here:
> https://www.weather.gov/wrh/timeseries?site=LLBG
>
> To toggle between Fahrenheit and Celsius, click the "Switch to Metric Units" button until the
> relevant table displays °C.
>
> … The resolution source for this market measures temperatures to whole degrees Celsius (eg, 9°C).

**Variant 3 — Hong Kong Observatory (Hong Kong only — note the *decimal* precision):**

> This market will resolve to the temperature range that contains the highest temperature recorded
> by the Hong Kong Observatory in degrees Celsius on 14 Aug '26.
>
> The resolution source for this market will be information from the Hong Kong Observatory,
> specifically the "Absolute Daily Max (deg. C)" the specified date once information is finalized
> in the relevant "Daily Extract", available here: https://www.weather.gov.hk/en/cis/climat.htm
>
> This market can not resolve until data for this date has been published.
>
> The resolution source for this market measures temperatures in Celsius to one decimal place (eg,
> 9.1°C). Thus, this is the level of precision that will be used when resolving the market.
>
> Any revisions to temperatures recorded after data is initially published for this market's
> timeframe will not be considered for this market's resolution.

### Timezone / measurement window

- **Not stated explicitly in the rules text.** The rules say "for all times on this day", which
  resolves to whatever calendar day the resolution source's own daily page uses.
- ✅ **Empirically the window is the local calendar day at the station.** Verified: the Weather
  Underground backing feed for `KLGA` on 2026-08-12 returns exactly 24 rows spanning
  `04:51Z → 03:51Z` (= 00:51–23:51 America/New_York). For `RJTT` it spans `2026-08-11T15:00Z →
  2026-08-12T14:30Z` (= 00:00–23:30 Asia/Tokyo). For `EGLC`: `23:20Z → 22:50Z` (= 00:20–23:50
  Europe/London).
- The Gamma `game_start_time` field is set to local midnight of the market date
  (NYC Aug 14 → `2026-08-14T04:00:00Z`), which corroborates a local-day window. The `endDate` /
  `end_date_iso` fields are **nominal placeholders** (`12:00:00Z` on the market date) and do *not*
  represent the trading cutoff — markets stayed `acceptingOrders: true` well past them.

### Rounding / ties

- Buckets are contiguous and non-overlapping integers, so **ties are structurally impossible** for
  the WU/NOAA variants — the source publishes one integer, one bucket contains it.
- The **precision of the source, not of the true temperature, is what settles.** For WU/NOAA
  variants that is *whole degrees* in the market's stated unit. For Hong Kong it is *tenths of °C*.
- **Rounding trap (⚠️ partially unverified, but empirically important):** the underlying ASOS
  METAR carries temperature in whole °C plus an `RMK T`-group in tenths. Converting the whole-°C
  value to °F and rounding can give a *different* bucket than what WU displays. For KLGA on
  2026-08-12: the 5-minute `api.weather.gov` feed peaked at `31.0 °C` (→ 87.8 °F → rounds to 88),
  the hourly METAR `T`-groups peaked at `30.0 °C` (→ 86 °F), the final NWS CLI said `87 °F`, and
  the market resolved to the **`86-87°F`** bucket. The WU feed itself reported `86 °F`. **Always
  read the resolution source's own numbers; never derive them by unit conversion.**
- **Revision window:** rules explicitly accept revisions "until the first datapoint for the
  following date has been published." Confirmed live that revisions happen — the preliminary CLI
  for KLGA 2026-08-12 (issued 20:34Z) said MAX `86`; the final CLI (issued 06:17Z next day) said
  `87`.

### Station map (✅ verified from live market descriptions — 51 daily series)

**These are traps.** "NYC" is LaGuardia, not Central Park. "Dallas" is Love Field, not DFW.
"Denver" is Buckley SFB, not Denver International. "Seoul" is Incheon. "Paris" is Le Bourget, not
CDG or Montsouris. "London" is London City, not Heathrow (but the *precipitation* market uses
Heathrow). "Karachi" is a military airbase.

| Series slug | Station | ICAO | Unit | Source |
|---|---|---|---|---|
| `nyc-daily-weather` / `nyc-daily-lowest-temperature` | LaGuardia Airport | KLGA | °F | WU |
| `chicago-daily-weather` | Chicago O'Hare Intl | KORD | °F | WU |
| `dallas-daily-weather` | Dallas **Love Field** | KDAL | °F | WU |
| `denver-daily-weather` | **Buckley Space Force Base** | KBKF | °F | WU |
| `los-angeles-daily-weather` | Los Angeles Intl | KLAX | °F | WU |
| `san-francisco-daily-weather` | San Francisco Intl | KSFO | °F | WU |
| `seattle-daily-weather` | Seattle-Tacoma Intl | KSEA | °F | WU |
| `miami-daily-weather` / `miami-daily-lowest-temperature` | Miami Intl | KMIA | °F | WU |
| `atlanta-daily-weather` | Hartsfield-Jackson Intl | KATL | °F | WU |
| `austin-daily-weather` | Austin-Bergstrom Intl | KAUS | °F | WU |
| `london-daily-weather` / `london-daily-lowest-temperature` | **London City Airport** | EGLC | °C | WU |
| `paris-daily-weather` / `paris-daily-lowest-temperature` | **Paris-Le Bourget** | LFPB | °C | WU |
| `amsterdam-daily-weather` | Amsterdam Schiphol | EHAM | °C | WU |
| `madrid-daily-weather` | Adolfo Suárez Madrid-Barajas | LEMD | °C | WU |
| `milan-daily-weather` | Malpensa Intl | LIMC | °C | WU |
| `munich-daily-weather` | Munich Airport | EDDM | °C | WU |
| `warsaw-daily-weather` | Warsaw Chopin | EPWA | °C | WU |
| `helsinki-daily-weather` | Helsinki Vantaa | EFHK | °C | WU |
| `ankara-daily-weather` | Esenboğa Intl | LTAC | °C | WU |
| `tokyo-daily-weather` / `tokyo-daily-lowest-temperature` | Tokyo **Haneda** | RJTT | °C | WU |
| `seoul-daily-weather` / `seoul-daily-lowest-temperature` | **Incheon Intl** | RKSI | °C | WU |
| `busan-daily-weather` | Gimhae Intl | RKPK | °C | WU |
| `shanghai-daily-weather` / `shanghai-daily-lowest-temperature` | Shanghai **Pudong** | ZSPD | °C | WU |
| `beijing-daily-weather` | Beijing Capital Intl | ZBAA | °C | WU |
| `guangzhou-daily-weather` | Guangzhou Baiyun Intl | ZGGG | °C | WU |
| `shenzhen-daily-weather` | Shenzhen Bao'an Intl | ZGSZ | °C | WU |
| `chengdu-daily-weather` | Chengdu Shuangliu Intl | ZUUU | °C | WU |
| `chongqing-daily-weather` | Chongqing Jiangbei Intl | ZUCK | °C | WU |
| `wuhan-daily-weather` | Wuhan Tianhe Intl | ZHHH | °C | WU |
| `qingdao-daily-weather` | Qingdao Jiaodong Intl | ZSQD | °C | WU |
| `zhengzhou-daily-weather` | Zhengzhou Xinzheng Intl | ZHCC | °C | WU |
| `jinan-daily-weather` | Jinan Yaoqiang Intl | ZSJN | °C | WU |
| `taipei-daily-weather` | Taipei **Songshan** | RCSS | °C | WU |
| `singapore-daily-weather` | Singapore Changi | WSSS | °C | WU |
| `kuala-lumpur-daily-weather` | Kuala Lumpur Intl | WMKK | °C | WU |
| `manila-daily-weather` | Ninoy Aquino Intl | RPLL | °C | WU |
| `karachi-daily-weather` | **Masroor Airbase** | OPKC | °C | WU |
| `lucknow-daily-weather` | Chaudhary Charan Singh Intl | VILK | °C | WU |
| `jeddah-daily-weather` | King Abdulaziz Intl | OEJN | °C | WU |
| `toronto-daily-weather` | Toronto Pearson Intl | CYYZ | °C | WU |
| `mexico-city-daily-weather` | Benito Juárez Intl | MMMX | °C | WU |
| `sao-paulo-daily-weather` | São Paulo-Guarulhos Intl | SBGR | °C | WU |
| `buenos-aires-daily-weather` | Minister Pistarini Intl | SAEZ | °C | WU |
| `cape-town-daily-weather` | Cape Town Intl | FACT | °C | WU |
| `wellington-daily-weather` | Wellington Intl | NZWN | °C | WU |
| `tel-aviv-daily-weather` | Ben Gurion Intl | LLBG | °C | **NOAA WRH** |
| `moscow-daily-weather` | **Vnukovo** Intl | UUWW | °C | **NOAA WRH** |
| `istanbul-daily-weather` | Istanbul Airport | LTFM | °C | **NOAA WRH** |
| `hong-kong-daily-weather` / `hong-kong-daily-lowest-temperature` | HK Observatory HQ | — | °C (0.1) | **HKO** |
| (also seen: Panama City, Sao Paulo lowest, etc.) | | | | |

WU history URL pattern: `https://www.wunderground.com/history/daily/{cc}/{city}/{ICAO}`

### ⭐ Machine-readable replication of the WU resolution source (✅ verified, 8/8 match)

`wunderground.com` is a JS SPA, but its backing JSON endpoint is directly callable and returns
**exactly the Daily Observations table**, on the correct local-day window:

```
GET https://api.weather.com/v1/location/{ICAO}:9:{ISO2}/observations/historical.json
      ?apiKey=e1f10a1e78da46f5b10a1e78da96f525
      &units=e            # e = imperial (°F), m = metric (°C)
      &startDate=20260812
      &endDate=20260812
```

Response: `{"observations":[{"valid_time_gmt":…,"temp":86,"obs_name":"New York/LaGuardia",…}]}`
→ take `max(.observations[].temp)` / `min(...)`.

**Backtest against 8 settled markets for 2026-08-12 (all 8 correct):**

| Market | Endpoint max/min | Resolved bucket |
|---|---|---|
| Highest temp NYC (`KLGA:9:US`, units=e) | 86 | `86-87°F` ✅ |
| Lowest temp NYC (`KLGA:9:US`, units=e) | 74 | `74-75°F` ✅ |
| Highest temp London (`EGLC:9:GB`, units=m) | 30 | `30°C` ✅ |
| Lowest temp London (`EGLC:9:GB`, units=m) | 18 | `18°C` ✅ |
| Highest temp Tokyo (`RJTT:9:JP`, units=m) | 29 | `29°C` ✅ |
| Highest temp Shanghai (`ZSPD:9:CN`, units=m) | 27 | `27°C` ✅ |
| Highest temp Moscow (`UUWW:9:RU`, units=m) | 18 | `18°C` ✅ |
| Highest temp Tel Aviv (`LLBG:9:IL`, units=m) | 35 | `35°C` ✅ |

Caveats, stated honestly: this endpoint is **undocumented**, the API key is one embedded in the
`wunderground.com` web client (not issued to you), and use is likely outside The Weather Company's
terms of service. It is excellent for *backtesting and cross-checking*; do not build a production
dependency on it without a licensed key. Note the Moscow and Tel Aviv markets nominally resolve
off `weather.gov/wrh/timeseries` rather than WU, yet the WU feed agreed for both — same upstream
METAR.

The `weather.gov/wrh/timeseries` page itself is HTML/Highcharts, backed by
`https://api.synopticdata.com/v2/stations/timeseries?stid=LLBG&recent=1440&vars=air_temp&token=…`.
The token embedded in `https://www.weather.gov/source/wrh/apiKey.js` is domain-restricted
(returns `403 "Invalid request per token rules"` when called directly — ✅ verified). Register a
free Synoptic Data "Open Access" token to use that path.

HKO machine-readable (✅ verified endpoint, lags):
```
GET https://data.weather.gov.hk/weatherAPI/opendata/opendata.php?dataType=CLMMAXT&year=2026&month=8&rformat=json&station=HKO
GET https://data.weather.gov.hk/weatherAPI/opendata/weather.php?dataType=rhrread&lang=en   # near-real-time, 0.1°C
```
`CLMMAXT` returned an empty `data` array for the in-progress month of Aug 2026 — it publishes with
a lag. `rhrread` returned `{"place":"Hong Kong Observatory","value":31,"unit":"C"}` at
`2026-08-14T00:02:00+08:00` (integer only in that product).

## A.3 Archetype 2 — Monthly precipitation totals

Event titles: `Precipitation in NYC in August?`, `Precipitation in London in August?`,
`Precipitation in Hong Kong in August?`, `Precipitation in Seoul in August?`,
`Precipitation in Seattle in August?` (5 live).

Outcomes (✅ verified): NYC `["<2\"","2-3\"","3-4\"","4-5\"","5-6\"",">6\""]`;
London `["<30mm","30-40mm","40-50mm","50-60mm","60-70mm","70-80mm","80mm+"]`;
Hong Kong uses 25 mm bands (`450-475mm`, `525-550mm`, …); Seoul uses 50 mm bands.

**Tie rule is explicit and different from the temperature markets:**

> If the reported value falls exactly between two brackets, then this market will resolve to the
> **higher** range bracket.

**NYC (✅ verbatim):**

> This market will resolve according to the total precipitation in inches in **Central Park**, New
> York City between August 1 and August 31, 2026, 11:59PM **ET** according to the National Oceanic
> and Atmospheric Administration (NOAA). … specifically the figure for August 2026 when the
> "Monthly summarized data" for "Central Park NY" is selected with the variable set to
> "Precipitation" at the https://www.weather.gov/wrh/climate?wfo=okx link once that figure is
> finalized for the whole month of August 2026. … If the relevant data is not available by
> November 7, 2026, 11:59 PM ET, another credible resolution source will be chosen. The resolution
> source … measures precipitation to 2 decimal places (e.g., 1.54).

⚠️ **Note the station switch:** NYC *temperature* resolves off **KLGA**; NYC *precipitation*
resolves off **Central Park (KNYC / GHCN `USW00094728`)**. Do not share a station config.

**London (✅ verbatim):**

> This market will resolve according to the total precipitation in mm at **Heathrow (London
> Airport)** in August, 2026, according to the **Met Office**. … specifically the figure for August
> 2026 under "rain mm" at the
> https://www.metoffice.gov.uk/pub/data/weather/uk/climate/stationdata/heathrowdata.txt link once
> the **Provisional** figure for the whole month of August 2026 is released. … measures
> precipitation to 1 decimal place (e.g., 1.5).

⚠️ London *temperature* = **EGLC** (London City); London *precipitation* = **Heathrow**. Different
station, different agency (Met Office vs WU), different unit convention.

The Met Office file is a plain fixed-width text file — trivially parseable and the single cleanest
resolution feed in the whole category.

## A.4 Archetype 3 — Global climate indices

| Event | Outcomes | Resolution source |
|---|---|---|
| `Where will 2026 rank among the hottest years on record?` | `1,2,3,4,5,6 or lower` | **NASA GISTEMP** `No_Smoothing` column, row `2026`, at `https://data.giss.nasa.gov/gistemp/graphs/graph_data/Global_Mean_Estimates_based_on_Land_and_Ocean_Data/graph.txt` |
| `August 2026 Temperature Increase (ºC)` | `<1.10ºC … >1.29ºC` (0.05 bands) | **NASA GISTEMP** `GLB.Ts+dSST.txt`, column `Aug`, row `2026`, at `https://data.giss.nasa.gov/gistemp/tabledata_v4/GLB.Ts+dSST.txt` |
| `Min Arctic sea ice extent this summer?` | 0.2 M km² bands | **NSIDC** Sea Ice Index Daily Extent, `NH-Daily-Extent` tab, `https://nsidc.org/sea-ice-today/sea-ice-tools` — min over 2026-08-01 → 2026-10-01 |
| `2026 August 1st, 2nd, 3rd hottest on record?` | rank | (GISTEMP family) |
| `Will summer 2026 be the UK's hottest summer on record?` | Yes/No | ⚠️ unverified — did not pull rules text (expect Met Office) |

Tie rule for the annual rank (✅ verbatim):

> If 2026 ties with any other year, it will resolve according to the place the year it ties with
> occupies.

Revision rule (✅ verbatim, and unusually harsh):

> This market will resolve **immediately** once the specified data becomes available, regardless of
> whether the figure for the relevant years is later revised.

For `August 2026 Temperature Increase`: if NASA publishes nothing by 2026-10-01 23:59 ET, the
market **resolves to the lowest bracket** — a hard-coded government-shutdown / data-outage risk.

## A.5 Archetype 4 — Tornado counts

`How many Tornadoes in the US in August 2026?` — outcomes `<100, 100–129, 130–159, … 310+`.
Also an annual variant (`1250+`, `1150–1199`, …).

Resolution (✅ verbatim, and precisely specified — good for an agent):

> …based on the monthly count published on the National Centers for Environmental Information U.S.
> Tornadoes Time Series page (see: https://www.ncei.noaa.gov/access/monitoring/tornadoes/time-series).
> Only tornadoes appearing in the final NCEI dataset for that month will count. As of market
> creation, the relevant report is scheduled to be released on **August 10, 2026, at 5:01 PM GMT+1
> or 11:00 AM ET** (Release schedule:
> https://www.ncei.noaa.gov/access/monitoring/dyk/monthly-releases). The market will resolve based
> on the **first** relevant tornado count published on the NCEI tornado time-series page **after**
> this scheduled release time. If the value published after this scheduled release time is labeled
> preliminary, it will still determine resolution, and the market will resolve independently of any
> subsequent revisions… The market will **not** resolve based on any preliminary values published
> before the scheduled release time.

This is a *data-release-timing* market, not a weather market. NCEI's preliminary monthly tornado
count is systematically well below the final count; the edge is in modelling the NCEI reporting
pipeline, not the atmosphere.

## A.6 Archetype 5 — Hurricanes / named storms

| Event | Outcomes | Source |
|---|---|---|
| `How many hurricanes will form during the Atlantic Hurricane Season in 2026?` | `0, 1-3, 4-6, 7+` | NHC advisories |
| `Will any Category 4 hurricane make landfall in the US in before 2027?` | Yes/No | NHC advisories |
| `Will 2 or more hurricanes make landfall in the US in 2026?` | Yes/No | NHC advisories |
| `What will be the name of the first hurricane in the Atlantic for the 2026 Hurricane Season?` | name list | NHC |
| `When will the first hurricane form in the Atlantic in 2026?` | date ranges + "No … will form" | NHC |
| `How many tropical cyclones will make landfall in China during 2026?` | integer buckets | ⚠️ unverified source |
| `What intensity will Typhoon Dolphin have at landfall in Japan?` | JMA intensity classes | ⚠️ likely JMA, unverified |

Key mechanics (✅ verbatim):

> A hurricane is a tropical cyclone with maximum sustained winds of 74 mph or greater (Category 1
> or higher on the Saffir-Simpson Hurricane Wind Scale), as described at
> https://www.nhc.noaa.gov/aboutsshws.php.
>
> This market may resolve based on the **initial NHC advisory** reporting a qualifying hurricane
> regardless of any later advisory, retraction, best-track revision, or reanalysis that revises
> that storm's intensity downward. The official count will be determined by the NHC (nhc.noaa.gov)
> advisories.

And for landfall (✅ verbatim):

> …a hurricane landfall is said to occur when the hurricane's surface center intersects with the
> coastline, as described at https://www.nhc.noaa.gov/aboutgloss.shtml#LANDFALL. … resolve to "Yes"
> if any storm makes landfall in the **conterminous** United States as a Category 4 hurricane …
> This market may resolve based on the initial advisory … regardless of any later retraction …
> However, subsequent corrections or updates **will** be considered if they indicate a qualifying
> incident.

**The ratchet is asymmetric and exploitable:** initial-advisory intensity counts for YES and cannot
be walked back, but later upgrades *can* create a YES. Real-time NHC advisory intensity is
noisier and biased differently from best-track. Trade the advisory, not the truth.

## A.7 Other archetypes present

- `Highest Mt. Washington wind speed in August?` — cumulative `≥85 mph … ≥115 mph` thresholds
  (note: **not** mutually exclusive buckets). Source: Mt. Washington Observatory **monthly F6
  reports**, `https://mountwashington.org/weather/mount-washington-weather-archives/monthly-f6/`,
  whole mph.
- `Will it rain during the Dutch Grand Prix?` — resolves off the **FIA Race Weather Report**
  `RAINFALL` trace registering `'Wet'` at any point during the Race session only. Cancelled/
  postponed past 2026-08-31 → **resolves 50-50**. Report not published by 2026-08-31 → resolves NO.
  Explicitly excludes race-control radio, commentary and news.
- Drought: `Will Oregon reach D4 (Exceptional Drought) by August 31, 2026?` (⚠️ US Drought Monitor
  presumed, unverified).
- ENSO: `Will there be a Super El Niño this winter (2026–27)?` (⚠️ source unverified).
- `Will Phoenix be under an Extreme Heat Warning for 10+ days in August 2026?` — an *NWS-product*
  market, not a temperature market.
- Rivers: `When will the Rhine River return to normal levels?`, Danube equivalent.
- Non-weather in-category: earthquakes (USGS), volcanoes (GVP), pandemics (WHO/CDC),
  `5kt meteor strike in 2026`, `Blue Origin New Glenn launch in 2026`.
- **No snowfall-threshold markets are live in August 2026.** ⚠️ Whether they exist in winter is
  unverified from this snapshot; the `-daily-weather` series machinery would support them.

## A.8 Market microstructure (✅ all verified on a live market)

Reference market: `highest-temperature-in-nyc-on-august-14-2026-86-87f`
(conditionId `0x91d2a043a00e19fa8af7befe79982d86d1b77f4fdc65ecc72871e2ddbdcaba5a`).

- **Instrument:** CLOB binary outcome tokens (ERC-1155 conditional tokens on Polygon), collateral
  **USDC** (`0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`), price ∈ (0,1), winner redeems at 1.00.
  `outcomes: ["Yes","No"]`, two `clobTokenIds` per market.
- **Multi-outcome:** implemented as a **negRisk (mutually-exclusive) group**, not a native
  multi-outcome book. `enableNegRisk: true`, shared `negRiskMarketID`. Each bucket is its own
  YES/NO book. The per-market `questionID` = `negRiskMarketID` with the low byte replaced by the
  outcome index (`…5b00`, `…5b01`, `…5b03`, …) — you can enumerate a whole ladder from one ID.
  negRisk lets you convert a basket of NOs into cash + the complementary YES, so the ladder's
  YES prices are arbitrage-linked to sum ≈ 1.
- **Tick size:** `0.01` on live daily temp markets (`orderPriceMinTickSize`, `minimum_tick_size`).
  Note: 2025-era weather markets used `0.001`. Read it per market, don't assume.
- **Min order size:** `5` shares (`orderMinSize` / `minimum_order_size`).
- **Liquidity (2026-08-13 snapshot):** the NYC Aug-14 *event* showed `liquidity: 57,484`,
  `openInterest: 3,486`; the single `86-87°F` market `liquidityNum: 11,607`. Live book on that
  token: bid 0.43×519, 0.42×408, 0.41×1252, 0.40×20,084; ask 0.44×181, 0.45×215, 0.46×458, 0.47×25.
  Spread `0.01`. So: **top-of-book depth is a few hundred dollars, with a large resting wall
  further out.** Event-level 24h volume ranged ~$5k (thin cities) to ~$110k (London).
  Series-level: `london-daily-weather` `volume24hr` ≈ $200k, `nyc-daily-weather` ≈ $75k.
- **Fees (✅ verified against docs + live market fields):** `feesEnabled: true`,
  `feeType: "weather_fees"`, `feeSchedule: {rate: 0.05, exponent: 1, takerOnly: true, rebateRate: 0.25}`.
  Formula: **`fee = C × feeRate × p × (1 − p)`** where `C` = shares, `p` = price.
  Weather taker rate **0.05**, maker rate **0**, maker rebate 25%. Peak fee is at p=0.50:
  **$1.25 per 100 shares ($50 notional) = 2.5% of notional.** Fees rounded to 5 dp, min 0.00001 USDC.
  → **This is a large drag on a 50/50 daily temperature bucket.** A strategy taking liquidity at
  mid on the modal bucket needs > ~2.5% edge just to break even. Post, don't take.
- **Liquidity rewards:** `rewards.min_size: 20`, `rewards.max_spread: 4.5` (¢),
  `rewards_daily_rate: 52` USDC/day on the reference market. Quoting inside 4.5¢ with ≥20 shares
  earns rewards — materially changes the economics vs. taking.
- **Resolution mechanism:** UMA optimistic oracle adapter, `resolvedBy`
  `0x69c47De9D4D3Dad79590d61b9e05918E03775f24`, `umaBond: 250`, `umaReward: 0.6`,
  `customLiveness: 900` (**15-minute** dispute window — much shorter than the 2-hour default, and
  shorter than the 2025 weather markets which used `customLiveness: 0` / `umaBond: 500`).
- **Resolution delay (✅ measured, 2026-08-12 daily markets):** typically **1–2 hours after local
  midnight** at the station.
  - `lowest-temperature-in-nyc-on-august-12` closed `2026-08-13T05:16:42Z` = 01:16 EDT (+1h16m)
  - `highest-temperature-in-chicago-on-august-12` closed `06:16:42Z` = 01:16 CDT (+1h16m)
  - `highest-temperature-in-london-on-august-12` closed `2026-08-12T23:49:10Z` = 00:49 BST (+49m)
  - `highest-temperature-in-shenzhen-on-august-12` closed `16:41:42Z` = 00:41 CST (+41m)
  - `highest-temperature-in-san-francisco-on-august-12` closed `08:20:10Z` = 01:20 PDT (+1h20m)
  Consistent with "cannot resolve until the first data point for the following date has been
  published" + 15 min UMA liveness. **Practical implication: ~45–90 min of post-event, pre-
  resolution time in which the answer is already knowable from the resolution source.**

## A.9 Polymarket read APIs (✅ every call below was executed successfully)

### Gamma API — `https://gamma-api.polymarket.com`

**List weather events (this is the primary discovery call):**
```
GET /events?tag_slug=weather&closed=false&limit=100&offset=0&order=volume24hr&ascending=false
```
Alternative by numeric tag: `?tag_id=84&related_tags=true`.
Useful params on `/events` and `/markets`: `limit`, `offset`, `order`, `ascending`, `id[]`,
`slug[]`, `clob_token_ids[]`, `condition_ids[]`, `liquidity_num_min/max`, `volume_num_min/max`,
`start_date_min/max`, `end_date_min/max`, `tag_id`, `related_tags`, `closed`, `question_ids[]`,
`include_tag`.

**Get one event with its full outcome ladder, prices and rules:**
```
GET /events?slug=highest-temperature-in-nyc-on-august-14-2026
```
Returns `[{ id, slug, title, description (← the resolution criteria), endDate, liquidity, volume,
openInterest, enableNegRisk, negRiskMarketID, seriesSlug, markets: [...], series: [...], tags: [...] }]`.

⚠️ **Slug gotcha:** current-year daily events are suffixed with the year
(`highest-temperature-in-nyc-on-august-14-2026`). The un-suffixed slug
`highest-temperature-in-nyc-on-august-14` resolves to the **2025** event — and the 2025 rules text
is materially different (e.g. London 2025 resolved in **°F**; London 2026 resolves in **°C**).
Always include the year and always re-read `description`.

**Get one bucket market:**
```
GET /markets?slug=highest-temperature-in-nyc-on-august-14-2026-86-87f
```
Resolution-relevant fields: `description`, `resolutionSource`, `resolvedBy`, `umaBond`,
`umaReward`, `customLiveness`, `umaResolutionStatus` (`null` → `"proposed"` → `"resolved"`),
`umaResolutionStatuses`, `closedTime`, `automaticallyResolved`, `questionID`, `conditionId`.
Pricing fields: `outcomes`, `outcomePrices`, `bestBid`, `bestAsk`, `spread`, `lastTradePrice`,
`oneHourPriceChange`, `oneDayPriceChange`, `liquidityNum`, `volumeNum`, `volume24hr`.
Trading params: `clobTokenIds`, `orderPriceMinTickSize`, `orderMinSize`, `rewardsMinSize`,
`rewardsMaxSpread`, `feesEnabled`, `feeType`, `feeSchedule`, `acceptingOrders`, `negRisk`.

**Settled-outcome read (for backtesting):** a settled market has
`outcomePrices == "[\"1\", \"0\"]"` for the winner. To get the winning bucket of an event:
```
GET /events?slug=highest-temperature-in-nyc-on-august-12-2026
# jq: .[0].markets | map(select(.outcomePrices=="[\"1\", \"0\"]") | .groupItemTitle)
```

**Series (recurring templates) and tags:**
```
GET /series?slug=nyc-daily-weather      # → {id:"10005", recurrence:"daily", volume24hr, liquidity}
GET /tags/slug/weather                   # → {id:"84", label:"Weather", slug:"weather"}
```

### CLOB API — `https://clob.polymarket.com` (all read endpoints are public, no auth)

```
GET /markets/{condition_id}
    → enable_order_book, accepting_orders, minimum_order_size, minimum_tick_size,
      question_id, market_slug, end_date_iso, game_start_time (= local midnight),
      maker_base_fee, taker_base_fee, neg_risk, neg_risk_market_id, neg_risk_request_id,
      rewards:{rates[{asset_address, rewards_daily_rate}], min_size, max_spread},
      tokens:[{token_id, outcome, price, winner}], tags:[...]

GET /book?token_id={token_id}
    → {market, asset_id, tick_size, neg_risk, min_order_size, bids:[{price,size}], asks:[...]}
      (bids ascending, asks descending — best bid is LAST in bids, best ask is LAST in asks)

GET /books                      # POST/GET batch variant for many token_ids
GET /price?token_id={id}&side=buy|sell
GET /midpoint?token_id={id}     → {"mid":"0.435"}
GET /spread?token_id={id}       → {"spread":"0.01"}
GET /prices-history?market={token_id}&interval=1d&fidelity=60
    → {"history":[{"t":1786586409,"p":0.355}, …]}   # t = unix seconds, fidelity = minutes
    # also accepts startTs / endTs instead of interval
GET /sampling-markets, /simplified-markets   # bulk market enumeration with next_cursor paging
```

`winner: true/false` on `/markets/{condition_id}` `tokens[]` is the authoritative on-chain
settlement flag. Writing orders requires L1/L2 auth headers (EIP-712 signed) — out of scope here.

## A.10 Design implications for a betting agent

1. **The tradable object is `P(source-reported integer ∈ bucket)`, not `P(true temperature)`.**
   You are forecasting a specific station's specific rounded reported value on a specific
   provider's table. Model source idiosyncrasy explicitly: METAR whole-°C quantisation, WU's
   °C→°F conversion behaviour, the WU-vs-CLI 1-degree discrepancies documented above.
2. **Metric cities are harder and better.** 1°C buckets ≈ 1.8°F — narrower than the 2°F US
   buckets — so the modal bucket rarely exceeds ~55%, and predictive edge is worth more.
3. **Fee structure forces market making.** 2.5%-of-notional taker fee at p≈0.5, maker fee 0,
   plus ~$52/day/market in liquidity rewards for quoting inside 4.5¢. A taker-only strategy needs
   an implausibly large edge on a next-day temperature bucket.
4. **negRisk gives you a hard arbitrage constraint.** YES prices across a ladder must sum to ~1;
   deviations are risk-free (minus fees) via NO-basket conversion. Cheap monitoring, real PnL.
5. **The 45–90 minute settlement lag is the cleanest edge in the category.** Between local
   midnight and UMA resolution, the resolution source's number is already published and readable
   via the endpoints in §A.2. This is deterministic, not predictive.
6. **The non-temperature archetypes are data-pipeline markets, not weather markets.** Tornado
   counts, GISTEMP anomalies and NHC advisory counts reward modelling *publication behaviour*
   (preliminary-vs-final bias, release schedules, advisory-vs-best-track divergence).
7. **Re-read `description` on every market, every day.** The template text changed materially
   between 2025 and 2026 (units, revision window, Daily-Observations-vs-Day-High-Low tiebreak).
   Cache station mappings but validate them against the live description before trading.

---

# PART B — Weather Data APIs for Programmatic Agent Use

## B.1 NOAA/NWS API — `https://api.weather.gov`

- **Auth:** none. A `User-Agent` header is **required** and should identify you, e.g.
  `User-Agent: (myweatherapp.com, contact@myweatherapp.com)`. Docs state this "will be replaced
  with an API key in the future."
- **Cost:** free, open data, no fees.
- **Rate limits (✅ verbatim from NWS docs):** *"The rate limit is not public information, but
  allows a generous amount for typical use. If the rate limit is exceeded a request will return
  with an error, and may be retried after the limit clears (typically within 5 seconds). Proxies
  are more likely to reach the limit."* No `X-RateLimit-*` headers are returned (✅ verified —
  response headers are only `server`, `x-request-id`, `x-correlation-id`, `x-server-id`,
  `server-timing`, `cache-control: public, max-age=86400, s-maxage=120`, `x-edge-request-id`).
- **Coverage:** **US only.** ✅ Verified that `EGLC`, `RJTT`, `LLBG`, `UUWW`, `LTFM` all return
  null/404 from `/stations/{id}/observations/latest`. Do **not** plan on api.weather.gov for
  international Polymarket cities.
- **Formats:** GeoJSON (default), `application/ld+json`, `application/vnd.noaa.dwml+xml`,
  `application/vnd.noaa.obs+xml`, CAP, ATOM via `Accept` header. All times ISO-8601.
- **OpenAPI spec:** `https://api.weather.gov/openapi.json`

### Endpoints that matter

```
GET /points/{lat},{lon}
    → properties.{gridId, gridX, gridY, forecast, forecastHourly, forecastGridData,
                 observationStations, timeZone}
    ✅ e.g. /points/40.7769,-73.8740 → gridId OKX, gridX 37, gridY 46, timeZone America/New_York

GET /gridpoints/{wfo}/{x},{y}/forecast          # 12h periods, ~7 days
GET /gridpoints/{wfo}/{x},{y}/forecast/hourly   # hourly, ~7 days
GET /gridpoints/{wfo}/{x},{y}                   # raw gridded forecast (maxTemperature, minTemperature, …)
    # ~2.5 km grid. Feature flags via header: Feature-Flags: forecast_temperature_qv, forecast_wind_speed_qv

GET /stations/{stationId}/observations/latest
GET /stations/{stationId}/observations?start={ISO8601}&end={ISO8601}&limit=500
    ✅ KLGA returns 311 obs per 24h (≈5-minute cadence). properties.temperature.unitCode is
      wmoUnit:degC. properties.rawMessage carries the full METAR incl. RMK T-group (tenths °C) —
      but only the ~17 hourly METARs have the T-group; the 5-min specials do not.
GET /gridpoints/{wfo}/{x},{y}/stations
```

⚠️ **`maxTemperatureLast24Hours` / `minTemperatureLast24Hours` are useless outside US Central time.**
✅ Verified null for KLGA, and NWS documents this as a known upstream bug: *"Station observations
endpoints always show missing (null) 24h max/min temperatures for stations outside the central time
zone due to MADIS ingest bug."* Also documented: observations may be **delayed up to 20 minutes**
from MADIS due to QC.

### ⭐ Pulling the *official* daily max/min for a US station (CLI climate report)

This is the authoritative NWS daily figure, and it is available via the API as text products:

```
GET /products/types/CLI/locations              # → all valid CLI location keys (incl. NYC, LGA, JFK, EWR, ORD, DFW)
GET /products?type=CLI&location=LGA&limit=4    # → [{id, issuanceTime, productName}]
GET /products/{id}                             # → {productText: "…"}
```

✅ Real output for `location=LGA`, product issued `2026-08-13T06:17Z`:

```
CLIMATE REPORT
NATIONAL WEATHER SERVICE NEW YORK, NY
217 AM EDT THU AUG 13 2026
...THE LAGUARDIA NY CLIMATE SUMMARY FOR AUGUST 12 2026...
CLIMATE NORMAL PERIOD 1991 TO 2020
CLIMATE RECORD PERIOD 1939 TO 2026
WEATHER ITEM   OBSERVED TIME   RECORD YEAR NORMAL DEPARTURE LAST
TEMPERATURE (F)
 YESTERDAY
  MAXIMUM         87    434 PM  98    2016  85      2       91
  MINIMUM         74    650 AM  56    1979  72      2       72
  AVERAGE         81                        78      3       82
PRECIPITATION (IN)
  YESTERDAY        0.00          6.40 1955   0.14  -0.14     0.00
```

Parse the `MAXIMUM` / `MINIMUM` rows plus the time-of-occurrence in **LST**. Note the record and
normal columns come free — directly useful for "hottest on record" style markets.

✅ **Revisions are real and observable:** two CLI products exist for LGA / Aug 12 2026 — the
20:34Z preliminary said `MAXIMUM 86 259 PM`, the 06:17Z final said `MAXIMUM 87 434 PM`. Always
take the latest issuance and record which one you saw.

Other useful product types: `CLM` (monthly climate), `RTP` (regional temp/precip roundup),
`F6` (preliminary monthly climatological data).

**Alerts** (relevant to the Phoenix "Extreme Heat Warning" market):
`GET /alerts/active?area={ST}` — 7-day rolling window; older alerts must come from NCEI.

## B.2 Open-Meteo — the workhorse for this use case

Base URLs (✅ all confirmed live):

| Purpose | Base URL |
|---|---|
| Deterministic forecast | `https://api.open-meteo.com/v1/forecast` |
| **Ensemble** | `https://ensemble-api.open-meteo.com/v1/ensemble` |
| Historical / ERA5 archive | `https://archive-api.open-meteo.com/v1/archive` |
| Historical forecast (reforecast archive) | `https://historical-forecast-api.open-meteo.com/v1/forecast` |
| Previous model runs | `https://previous-runs-api.open-meteo.com/v1/forecast` |
| Climate projections | `https://climate-api.open-meteo.com/v1/climate` |
| Seasonal | `https://seasonal-api.open-meteo.com/v1/seasonal` |
| Flood, Marine, Air Quality, Geocoding, Elevation, Satellite Radiation | `https://{flood,marine,air-quality,geocoding,customer}-api.open-meteo.com/...` |
| Commercial | `https://customer-api.open-meteo.com/v1/...` + `&apikey=…` |

- **Auth:** none on the free tier. Commercial tier adds `&apikey=` on `customer-api.*`.
- **Free-tier terms (✅ verbatim from the pricing page):** **non-commercial use only**;
  **600 calls/min, 5,000/hour, 10,000/day, 300,000/month**; no uptime guarantee. Server code is
  AGPLv3; the data is CC BY 4.0 and **attribution is required**. Ensemble, Historical, Historical
  Forecast, Previous Runs, Single Runs, Climate, Seasonal and Satellite Radiation APIs are
  available on the free tier and on **Professional and above** — but explicitly **NOT** on the
  paid *Standard* plan (an easy trap if you upgrade).
- **Call accounting:** ">10 weather variables or >2 weeks for a single location" counts as
  multiple calls, fractionally. 2 weeks × 15 variables = 1.5 calls.

### ⭐ Ensemble API — verified member counts

```
GET https://ensemble-api.open-meteo.com/v1/ensemble
    ?latitude=40.7769&longitude=-73.8740
    &models=gfs025,icon_eu,ecmwf_ifs025,gem_global,bom_access_global_ensemble
    &daily=temperature_2m_max,temperature_2m_min
    &hourly=temperature_2m
    &forecast_days=7
    &temperature_unit=fahrenheit          # or celsius (default)
    &timezone=America/New_York            # CRITICAL: makes daily aggregation the LOCAL calendar day
```

Response keys are `{variable}_member{NN}_{model}` (plus one unsuffixed control/deterministic key
per model). ✅ Verified per-model member counts on a single request:

| `models=` value | Underlying system | Series returned |
|---|---|---|
| `ecmwf_ifs025` | ECMWF IFS ENS 0.25° | **51** |
| `icon_eu` | DWD ICON-EU EPS | **40** |
| `gfs025` | NCEP GEFS 0.25° | **31** |
| `gem_global` | ECCC GEPS | **21** |
| `bom_access_global_ensemble` | BoM ACCESS-GE | **18** |

Total **161 members** in one HTTP call, ~0.35 ms server generation time. Also available:
`icon_global`, `icon_d2`, `gfs05`, `ukmo_global_ensemble_20km`, `ukmo_uk_ensemble_2km`,
`meteofrance_arpege_europe`/`arome_france`, `dmi_harmonie_ens`, `knmi_harmonie`, `metno_nordic`,
`jma_msm`/`gsm`, `kma_gdps`, `cma_grapes_global`, `gem_hrdps_continental`.

For a Polymarket bucket market, `temperature_2m_max` across all 161 members with
`timezone=<station tz>` and `temperature_unit=<market unit>` gives you a direct empirical
predictive distribution over the bucket ladder. This is the single highest-value endpoint for
this application. **Do calibrate:** ensemble grid-cell 2 m T ≠ station-reported rounded T. Fit a
bias+spread correction per station against verified history (§B.3, §A.2).

**Deterministic AI models on Open-Meteo (✅ tested):**
- `models=ecmwf_aifs025_single` → **returns real data** (verified: `[26.9, 26.4, 25.6]` °C).
- `models=ecmwf_aifs025` and `models=gfs_graphcast025` → accepted as valid model names but
  returned `null` for `temperature_2m` at test time (likely renamed/retired aliases). An invalid
  name like `bogus_model_xyz` returns a different shape entirely, so these are recognised —
  just empty. Treat as ⚠️ unreliable; use `ecmwf_aifs025_single`.

### Historical / ERA5 archive (✅ verified)

```
GET https://archive-api.open-meteo.com/v1/archive
    ?latitude=40.7769&longitude=-73.8740
    &start_date=2025-08-12&end_date=2025-08-14
    &daily=temperature_2m_max,temperature_2m_min,precipitation_sum
    &temperature_unit=fahrenheit&timezone=America/New_York
    &models=era5                 # also: era5_land, era5_ensemble, ecmwf_ifs, cerra
```
✅ Returned `temperature_2m_max: [84.0, 85.2, 85.2]`, `precipitation_sum: [0.00, 0.30, 4.70]` (mm).
ERA5 from 1940; ~5-day latency (ERA5T). **Note this is reanalysis grid data, not station data** —
for a station-truth backtest use NCEI/GHCN (§B.3) or the WU feed (§A.2), not ERA5.

### Previous Model Runs / Historical Forecast

`https://previous-runs-api.open-meteo.com/v1/forecast` exposes prior runs via
`{variable}_previous_day{N}` suffixes. ⚠️ My test with
`daily=temperature_2m_max_previous_day1` returned `null` — I did not determine the correct
variable spelling for daily aggregates. `historical-forecast-api` archives past forecasts from
2021 onward and is the right tool for building a forecast-verification / bias-correction dataset.
Both require the **Professional** plan for commercial use.

## B.3 NOAA NCEI / GHCN-Daily — official daily records & verification

**Access Data Service (no auth, no key — ✅ verified):**
```
GET https://www.ncei.noaa.gov/access/services/data/v1
    ?dataset=daily-summaries
    &stations=USW00014732                 # GHCN id for KLGA
    &startDate=2026-08-01&endDate=2026-08-12
    &dataTypes=TMAX,TMIN,PRCP,SNOW,SNWD,AWND,WSFG
    &units=standard                       # or metric
    &format=json                          # or csv, pdf, netcdf
    [&boundingBox=lat_n,lon_w,lat_s,lon_e] [&includeAttributes=true] [&includeStationName=true]
```
✅ Sample response: `[{"DATE":"2026-08-01","STATION":"USW00014732","TMAX":"87","TMIN":"74","PRCP":"0.00"}, …]`

**Search / metadata service:**
```
GET https://www.ncei.noaa.gov/access/services/search/v1/data?dataset=daily-summaries&stations=USW00014732&limit=1
    → per-dataType coverage %, dateRange.start/end
```

- **Auth:** none for the Access Data Service. The older **CDO API v2**
  (`https://www.ncei.noaa.gov/cdo-web/api/v2/data`) requires a free `token` header and is limited
  to **5 requests/sec, 10,000 requests/day**.
- ✅ **Latency measured:** on 2026-08-13, `daily-summaries` for USW00014732 had data only through
  **2026-08-10** — a ~2–3 day lag. **GHCN-Daily is therefore useless for same-day Polymarket
  resolution and ideal for backtesting.**
- Also relevant: `dataset=global-summary-of-the-day` (GSOD, international stations),
  `dataset=global-hourly` (ISD, international hourly METAR archive back to 1901 — the right
  source for reconstructing non-US station daily extremes historically),
  `dataset=normals-daily` (1991–2020 climate normals, great priors for bucket base rates).
- Tornado counts for the tornado markets:
  `https://www.ncei.noaa.gov/access/monitoring/tornadoes/time-series` +
  release calendar at `https://www.ncei.noaa.gov/access/monitoring/dyk/monthly-releases`.
- Station id crosswalk: `https://www.ncei.noaa.gov/pub/data/ghcn/daily/ghcnd-stations.txt`
  (GHCN id ↔ lat/lon/name; US ASOS ids also appear in `ghcnd-stations.txt` with the ICAO in the
  name field).

Useful GHCN ids for the Polymarket cities: KLGA `USW00014732`, Central Park (NYC precip)
`USW00094728`, KORD `USW00094846`, KDAL `USW00013960`, KLAX `USW00023174`, KSFO `USW00023234`,
KSEA `USW00024233`, KMIA `USW00012839`, KATL `USW00013874`, KAUS `USW00013904`, KBKF `USW00093067`.
⚠️ These are from memory, **not verified in this session** except `USW00014732`. Validate each
against `ghcnd-stations.txt` before use.

## B.4 ECMWF Open Data — `https://data.ecmwf.int/forecasts`

- **Auth:** none. **Cost:** free.
- **Licence:** CC-BY-4.0 + ECMWF Terms of Use. Redistribution and **commercial use permitted**
  with attribution. DOI `10.21957/open-data`.
- **Limits (✅ verbatim from ECMWF):** *"access to the Open-Data Portal is currently limited to
  **500 simultaneous connections**"*; *"Data are retained for the most recent **12 forecast runs**,
  corresponding to approximately **2–3 days**"*. Higher resolution requires a paid Real-time
  Dissemination Service Agreement.
- **Mirrors:** AWS, Azure, GCP. The `ecmwf-opendata` Python client takes
  `source="ecmwf"|"aws"|"azure"|"google"`.
- **Resolution:** 0.25°, GRIB2 with CCSDS compression.
- **Release timing:** IFS is released *at the end of* the real-time dissemination schedule;
  **AIFS is released as soon as it is produced** (i.e. AIFS is available earlier than IFS).

### URL pattern (✅ every 200 below was confirmed by HTTP HEAD on 2026-08-13)

```
https://data.ecmwf.int/forecasts/{YYYYMMDD}/{HH}z/{model}/0p25/{stream}/{YYYYMMDD}{HH}0000-{step}h-{stream}-{type}.grib2
```

| Path | Status | Size |
|---|---|---|
| `20260813/00z/ifs/0p25/oper/20260813000000-24h-oper-fc.grib2` | ✅ 200 | 145.6 MB |
| `20260813/00z/ifs/0p25/enfo/20260813000000-24h-enfo-ef.grib2` | ✅ 200 | **6.73 GB** |
| `20260813/00z/aifs-single/0p25/oper/20260813000000-24h-oper-fc.grib2` | ✅ 200 | 85.7 MB |
| `20260813/00z/aifs-single/0p25/wave/20260813000000-24h-wave-fc.grib2` | ✅ 200 | 8.2 MB |
| `20260813/00z/aifs-ens/0p25/enfo/20260813000000-24h-enfo-cf.grib2` | ✅ 200 | 89.0 MB |
| `20260813/00z/aifs-ens/0p25/enfo/20260813000000-24h-enfo-pf.grib2` | ✅ 200 | — |
| `20260812/12z/aifs-ens/0p25/enfo/20260812120000-24h-enfo-cf.grib2` | ✅ 200 | — |
| `…/aifs-ens/0p25/enfo/…-enfo-ef.grib2` | ❌ 404 | (use `cf`/`pf`, not `ef`) |
| `…/ifs/0p25/enfo/…-enfo-ep.grib2` and `…-enfo-em.grib2` | ❌ 404 | ⚠️ different naming for `ep`/`em`/`es`; ranged products use a `{a}-{b}h` step form I did not pin down |

Suffix legend: `fc` control forecast · `cf` ensemble control · `pf` perturbed members ·
`ef` ensemble (IFS combined) · `em` mean · `es` std dev · `ep` probabilities · `tf` TC tracks (BUFR).

**GRIB index sidecar (✅ verified) — this is how you avoid the 6.7 GB download.** Replace
`.grib2` with `.index` to get newline-delimited JSON with byte offsets, then HTTP Range-GET only
the messages you want:

```
GET https://data.ecmwf.int/forecasts/20260813/00z/ifs/0p25/oper/20260813000000-24h-oper-fc.index
{"domain":"g","date":"20260813","time":"0000","expver":"0001","class":"od","type":"fc",
 "stream":"oper","levtype":"sfc","step":"24","param":"tp","_offset":0,"_length":829365}
```

### Parameters relevant to temperature markets (from the official catalogue)

- IFS `oper`/`enfo`: `2t` (167), **`mx2t3` (228026)** / **`mn2t3` (228027)** max/min 2 m T in last
  3 h, `mx2t6` (121) / `mn2t6` (122) in last 6 h, `tp` (228), `sf` (144), `10fg`, `mucape`, `msl`.
  Steps 00z/12z: 0–144 by 3, then 150–360 by 6. 06z/18z: 0–144 by 3 (as of Cycle 50r1, `scda`/`scwv`
  are gone; 06z/18z now live under `stream=oper` / `stream=wave`).
- IFS `enfo` `type=ep` daily probabilities: `tpg1/5/10/20/25/50/100` (precip ≥ N mm),
  `10fgg10/15/25` (gust ≥ N m/s), ranges 0-24 → 336-360 by 12.
- **AIFS Single** (`model=aifs-single; class=ai; stream=oper; type=fc`): 4 runs/day, **6-hourly
  steps to 360 h**. Has `2t`, `2d`, `tp`, `sf`, `cp`, `tcc`, `msl`, `10u/10v`, `100u/100v`, `skt`,
  soil. ⚠️ **No `mx2t3`/`mn2t6`** — AIFS gives instantaneous 6-hourly `2t` only, so a daily max
  from AIFS open data is `max` over 6-hourly snapshots, which **systematically underestimates the
  true diurnal peak**. Material for these markets.
- **AIFS ENS** (`model=aifs-ens; class=ai; stream=enfo; type=pf/cf`): same fields, plus
  `type=em/es` means/std devs (incl. surface `2t`, `10si`, `100si`) and `type=ep` probability
  products (`tpg*`, `2tl273`, `10spg10/15`).
- TC tracks (BUFR) for hurricane markets: IFS `stream=oper|enfo, type=tf` and AIFS
  `aifs-single`/`aifs-ens` `type=tf`, out to step 360.
- Seasonal `sst`: listed as **not yet available as of August 2026**.

## B.5 Free access to AI weather model outputs — honest cost/access assessment

| Model | Access | Cost | Verdict |
|---|---|---|---|
| **ECMWF AIFS Single / AIFS ENS** | `https://data.ecmwf.int/forecasts/{date}/{HH}z/aifs-single\|aifs-ens/0p25/{oper\|enfo\|wave\|waef}/…` + AWS/Azure/GCP mirrors | **Genuinely free**, CC-BY-4.0, commercial use OK, no key | ✅ **Best free AI-model option.** 0.25°, 4 runs/day, 6-hourly to 15 days, ensemble included, index sidecars for byte-range fetch. Rolling 2–3 day archive only. |
| **ECMWF AIFS via Open-Meteo** | `api.open-meteo.com/v1/forecast?models=ecmwf_aifs025_single` | Free tier (non-commercial) | ✅ Verified working. Zero-infrastructure way to get AIFS at a point. |
| **Google DeepMind WeatherNext 2** | BigQuery (`bigquery-public-data`-style via Analytics Hub), Earth Engine (`ee.ImageCollection("projects/gcp-public-data-weathernext/assets/weathernext_2_0_0")`), and GCS Zarr `gs://weathernext/weathernext_2_0_0/zarr` (+ `_mean`) | **Access is gated by an application form** ("WeatherNext Data Request form"). Data itself: historical (>48 h old) **CC BY 4.0**; real-time (<48 h) under *GDM Real-Time Weather Forecasting Experimental Data Terms of Use*. You pay **Google Cloud compute/query costs** — BigQuery public-dataset model is "first 1 TB/month of query free, then standard query pricing"; Earth Engine commercial use requires a paid EE licence. | ⚠️ **Not "free and open" in the ECMWF sense.** Requires an approved data request + a GCP billing account. 64 ensemble members, 0.25°, 6-hourly init (00/06/12/18), 6-hourly leads to 15 days, dataset availability from 2022-01-01. Dissemination: 00z→07:30 UTC, 06z→13:30, 12z→19:30 (±15 min typical). **That ~7.5 h lag is slow for a next-day temperature market.** Explicitly labelled "experimental". |
| **Microsoft Aurora / Aurora-1.5** | Azure AI Foundry model catalogue (`ai.azure.com/catalog/models/Aurora`, `/Aurora-1.5`); Python client `aurora.foundry.{FoundryClient, BlobStorageChannel, submit}` | **Not a data feed — a model endpoint you deploy and pay for.** You provision an Azure Foundry endpoint (GPU compute, A100-class), supply your own initial conditions (e.g. HRES T0), and pay Azure compute + blob storage. Lifecycle: **Preview**. Commercial use requires contacting `AIWeatherClimate@microsoft.com`. Open-source weights + code on GitHub (`microsoft/aurora`) for self-hosting. | ⚠️ **Not free, and not a forecast archive.** You are running inference yourself. Aurora 1.5 adds 22 new single-level outputs (radiation, precip, 100 m winds), variable lead-time embeddings down to **1-hour resolution**, and a stochastic ensemble variant. The 1-hour lead-time capability is genuinely interesting for daily-max markets, but the operational burden (sourcing HRES T0 initial conditions + GPU cost) is high. |
| **GraphCast (NOAA/NCEP GFS-GraphCast)** | Open-Meteo `models=gfs_graphcast025`; also NOAA NODD on AWS/GCS | Free | ⚠️ Open-Meteo returned `null` for `temperature_2m` in my test. Treat as unreliable via that route; go to NODD S3 if you need it. |
| **Huawei Pangu-Weather, NVIDIA FourCastNet** | Open weights; no free operational feed | Self-host | ⚠️ Not verified this session. |

**Bottom line for an agent on a budget: ECMWF AIFS open data (free, commercial-OK, no key) plus
Open-Meteo's 161-member multi-model ensemble covers ~all of the realistic edge. WeatherNext needs
an approved request + GCP billing and is 7.5 h late. Aurora is a compute product, not a data
product.**

## B.6 Meteostat

- **Bulk (the actually-free path — ✅ verified):**
  `https://bulk.meteostat.net/v2/daily/{station}.csv.gz` — e.g. `.../v2/daily/72503.csv.gz`
  returned **HTTP 200, 333 KB**. No auth. Also `/v2/hourly/{station}/{year}.csv.gz`,
  `/v2/monthly/{station}.csv.gz`, `/v2/normals/{station}.csv.gz`,
  `/v2/stations/full.json.gz`, `/v2/stations/lite.json.gz`.
- **Python library** `meteostat` (`Daily`, `Hourly`, `Point`, `Stations`) reads the bulk endpoints
  directly — no key needed.
- **JSON API:** `https://meteostat.p.rapidapi.com/…` via **RapidAPI**, key required.
  ✅ Verified: unauthenticated request returns **HTTP 401**. Free RapidAPI tier exists with a
  monthly quota; paid tiers above.
- **Licence:** ⚠️ Meteostat data is generally **CC BY-NC 4.0** (non-commercial) with underlying
  DWD/NOAA sources under their own terms. I did not re-verify the exact licence text this session.
  **If you are trading real money, treat Meteostat as non-commercial and use NCEI/ISD instead.**
- **Unique value:** pre-merged, gap-filled, interpolated **global** station daily/hourly series
  with a uniform schema — by far the least-effort way to build a long station-history backtest for
  the non-US Polymarket cities. Station id `72503` = LaGuardia (WMO-style ids; ICAO lookup via the
  stations JSON).

## B.7 Visual Crossing

- **Base:** `https://weather.visualcrossing.com/VisualCrossingWebServices/rest/services/timeline/{location}/{start}/{end}?unitGroup=us|metric&key={KEY}&include=days,hours,obs&elements=tempmax,tempmin,precip`
- **Auth:** API key, required.
- **Free tier:** 1,000 "records"/day (a 15-day forecast query = 1 record; each history day = 1 record).
- **Paid tiers (✅ from the pricing page):** Professional — 10 M records/month, 10,000
  records/query, 100 locations/query, **concurrency 1**; Metered — unlimited records, 25,000/query,
  5 locations/query, unlimited concurrency; Corporate — 25,000/query, 1,000 locations/query,
  concurrency 10, plus sub-hourly history, agricultural/energy/maritime elements, degree days,
  history-summary reporting, OData, no attribution required. Uptime 99.9%.
  **Attribution ("Weather Data Provided by Visual Crossing") is required on Pay-as-you-go and Pro.**
- **Unique value:** a *single* Timeline endpoint that transparently blends history + forecast with
  one schema, 50+ years of history, and **severe-weather event records (hail, tornadoes)**. The
  cleanest commercial fill-in for non-US station history when Meteostat's NC licence is a blocker.

## B.8 OpenWeatherMap

- **Base:** `https://api.openweathermap.org/data/2.5/weather`, `/forecast`,
  `https://api.openweathermap.org/data/3.0/onecall`, `/data/3.0/onecall/timemachine`,
  and the new **One Call 4.0** timeline product.
- **Auth:** `appid={KEY}`, required.
- **Free (✅ from the pricing page):** "Free for everyone" — **60 calls/min, 1,000,000 calls/month**
  covering Current Weather, 3-hour/5-day Forecast, Air Pollution, Weather Maps (15 layers),
  Geocoding. **Free for students** adds Weather History, Statistical Weather Data, Accumulated
  Parameters, Hourly Forecast (4 days), Daily Forecast (16 days). One Call 4.0 is pay-per-call
  with **first 1,000 calls/day free**.
- **Paid tiers:** Startup 600 calls/min & 10 M/month; Developer 3,000/min & 100 M/month;
  Professional 30,000/min & 1 B/month; Expert 100,000/min & 3 B/month. Licence **ODbL**.
  Update frequency: every 2 h (Startup) → every 10 min (Professional/Expert).
  Availability 95% → 99.9%.
- **History bulk** from 1979-01-01; **History-forecast bulk** (archived 16-day forecasts as
  issued) from 2017-10-07 — the latter is genuinely useful for forecast-verification work.
- **Verdict for this use case: skip it.** OWM is a blended/proprietary point forecast with no
  ensemble, no station-level "official daily max", and an ODbL licence. Open-Meteo dominates it on
  every axis that matters for bucket-probability estimation.

## B.9 Recommended stack

| Job | Tool |
|---|---|
| Market discovery, rules text, ladders, prices | Gamma `/events?tag_slug=weather&closed=false` + `/events?slug=…` |
| Order book, midpoint, price history | CLOB `/book`, `/midpoint`, `/prices-history` |
| **Predictive distribution over buckets** | Open-Meteo `ensemble-api` — 161 members across 5 systems, `timezone=<station tz>`, `temperature_unit=<market unit>` |
| Secondary / AI-model signal | ECMWF `aifs-single` + `aifs-ens` open data (free, commercial-OK), via `.index` byte-range fetch |
| Nowcast / intraday state | `api.weather.gov/stations/{ICAO}/observations` (US, 5-min); Synoptic Data timeseries (global, own token) |
| **Settlement-source truth (the deterministic edge)** | `api.weather.com/v1/location/{ICAO}:9:{CC}/observations/historical.json` (WU-backed, ✅ 8/8 backtest) — cross-checked against NWS CLI products for US |
| Official US daily max/min + records + normals | `api.weather.gov/products?type=CLI&location={id}` |
| Station-truth backtest history | NCEI `daily-summaries` (US), `global-hourly`/ISD (international), `normals-daily` for base rates |
| Monthly precip resolution | Met Office `heathrowdata.txt` (London); NCEI/`weather.gov/wrh/climate?wfo=okx` (NYC Central Park) |
| Global climate index markets | NASA GISTEMP `GLB.Ts+dSST.txt` & `graph.txt`; NSIDC Sea Ice Index |
| Hurricane markets | NHC advisories + ECMWF/AIFS TC-track BUFR (`type=tf`) |
| Tornado markets | NCEI tornado time-series + monthly release calendar |

### Non-obvious risks to encode

1. **Never derive the settlement number by unit conversion.** Read the source's own integer.
   Documented 1–2 degree divergence between the 5-min METAR peak, the hourly METAR `T`-group, the
   preliminary CLI, and the final CLI for the same station-day.
2. **Station identity is per-market and per-variable.** NYC temp = KLGA, NYC precip = Central Park.
   London temp = EGLC, London precip = Heathrow.
3. **Units are per-city, not per-region.** US cities in °F with 2° buckets; everything else in °C
   with 1° buckets; Hong Kong in 0.1 °C.
4. **The 2.5%-of-notional taker fee at p≈0.5 will eat a naive strategy.** Quote inside 4.5¢ for
   rewards instead.
5. **negRisk ladder YES prices must sum to ~1** — a free, continuously-monitorable constraint.
6. **Rules text mutates between seasons.** Re-parse `description` daily; the 2025→2026 template
   change flipped London from °F to °C and added the Daily-Observations-vs-Day-High-Low tiebreak.
7. **Data-outage clauses are load-bearing.** GISTEMP August market resolves to the *lowest*
   bracket if NASA publishes nothing by 2026-10-01. A US government shutdown is a tradeable event
   in this category.
