# Risk Monitor Agent

**Type:** Risk Assessment  
**Executor:** MCP (Multi-Context Protocol)  
**Tier:** Specialist  
**Model:** Claude Sonnet 4.5 @ 0.2 temperature

## Overview

The Risk Monitor agent assesses security vulnerabilities and threat intelligence for software dependencies and infrastructure components. It queries the National Vulnerability Database (NVD), MITRE ATT&CK framework, and GitHub Security Advisories to provide evidence-based risk ratings.

## Key Capabilities

- CVE vulnerability lookups and analysis
- MITRE ATT&CK threat technique mapping
- Risk scoring (Critical/High/Medium/Low)
- Exploitability assessment
- Mitigation recommendations
- Threat actor attribution
- Dependency vulnerability tracking

## MCP Servers

### 1. CVE Database (Required)
- **Source:** National Vulnerability Database (NVD)
- **Endpoint:** `https://services.nvd.nist.gov/rest/json/cves/2.0`
- **Auth:** API key required
- **Rate Limit:** 5000 requests/day
- **Purpose:** CVE lookups, CVSS scoring, vulnerability details

### 2. Threat Intelligence (Required)
- **Source:** MITRE ATT&CK Framework
- **Endpoint:** `https://attack.mitre.org/api/`
- **Auth:** None (public API)
- **Rate Limit:** Unlimited
- **Purpose:** Threat techniques, tactics, procedures (TTPs)

### 3. GitHub Security Advisories (Optional)
- **Source:** GitHub Security Advisory Database
- **Endpoint:** `https://api.github.com/advisories`
- **Auth:** Bearer token (GitHub PAT)
- **Rate Limit:** 5000 requests/hour
- **Purpose:** Open source dependency vulnerabilities

## Ontology Structure

The agent builds a comprehensive security ontology tracking:

### Core Entities

**VULNERABILITY** (156 entities)
- CVE identifiers, CVSS scores, exploitability metrics
- Links to affected products, mitigations, threat actors

**AFFECTED_PRODUCT** (89 entities)
- Software products, versions, vendors
- Infrastructure components, dependencies

**THREAT_ACTOR** (23 entities)
- Known threat actors, APT groups, sophistication levels
- Attack techniques and motivations

**MITIGATION** (112 entities)
- Security controls, patches, workarounds
- Effectiveness ratings, implementation costs

**RISK_ASSESSMENT** (1247 entities)
- Risk scores, justifications, confidence levels
- Historical assessments and trends

### Key Relationships

```
VULNERABILITY → affects → AFFECTED_PRODUCT
VULNERABILITY → has → MITIGATION
VULNERABILITY → exploited_by → THREAT_ACTOR
THREAT_ACTOR → uses → ATTACK_TECHNIQUE
ATTACK_TECHNIQUE → implements → TACTIC
MITIGATION → implemented_via → CONTROL
```

## Usage Examples

### Query 1: CVE Risk Assessment
```
Query: "Assess the risk of CVE-2024-1234 for our Node.js infrastructure"

Agent Process:
1. Queries NVD for CVE-2024-1234 details
2. Extracts CVSS score, exploitability metrics
3. Checks MITRE ATT&CK for related techniques
4. Queries GitHub for affected npm packages
5. Analyzes mitigation availability
6. Generates risk assessment with confidence score

Output:
{
  "risk_level": "High",
  "risk_score": 7.8,
  "confidence": 0.92,
  "justification": "CVE-2024-1234 affects Express.js <4.18.2 with CVSS 7.5. Active exploitation observed via HTTP Request Smuggling (T1499). Patch available since 2024-01-15.",
  "mitigation": "Upgrade to Express.js >=4.18.2",
  "exploitability": "High (public PoC available)",
  "affected_versions": ["express@4.17.0 - 4.18.1"]
}
```

### Query 2: Dependency Vulnerability Scan
```
Query: "What critical vulnerabilities affect our Python dependencies?"

Agent Process:
1. Scans dependency tree
2. Cross-references with NVD database
3. Identifies critical CVEs (CVSS >= 9.0)
4. Checks for available patches
5. Prioritizes by exploitability

Output:
{
  "critical_vulnerabilities": [
    {
      "package": "requests",
      "version": "2.25.1",
      "cve": "CVE-2024-5678",
      "cvss": 9.1,
      "risk_level": "Critical",
      "mitigation": "Upgrade to requests>=2.31.0"
    }
  ],
  "total_vulnerabilities": 12,
  "confidence": 0.95
}
```

### Query 3: Threat Actor Attribution
```
Query: "Which threat actors are known to exploit Log4Shell?"

Agent Process:
1. Queries MITRE ATT&CK for Log4Shell (CVE-2021-44228)
2. Identifies associated threat actors
3. Maps to attack techniques used
4. Provides threat context

Output:
{
  "vulnerability": "CVE-2021-44228 (Log4Shell)",
  "threat_actors": [
    {
      "name": "APT41",
      "sophistication": "High",
      "motivation": "Financial, Espionage",
      "techniques": ["T1190 (Exploit Public-Facing Application)", "T1059 (Command Execution)"]
    }
  ],
  "confidence": 0.88
}
```

## Risk Scoring Methodology

### Risk Level Calculation
```
Risk Score = (CVSS Base Score × 0.4) + 
             (Exploitability × 0.3) + 
             (Threat Context × 0.2) + 
             (Mitigation Availability × 0.1)

Risk Levels:
- Critical: 9.0 - 10.0
- High: 7.0 - 8.9
- Medium: 4.0 - 6.9
- Low: 0.0 - 3.9
```

### Confidence Factors
- **High (0.9+)**: Multiple authoritative sources, confirmed CVE
- **Medium (0.7-0.89)**: Single authoritative source, unconfirmed reports
- **Low (<0.7)**: Unverified reports, limited evidence

## Performance Metrics

- **Accuracy Rate:** 91% (validated against security team assessments)
- **Average Confidence:** 0.88
- **Execution Count:** 1,247 assessments
- **Average Execution Time:** 3.5 seconds
- **Last Calibration:** 2026-01-15

## Configuration

### Environment Variables
```bash
# Required
NVD_API_KEY=your_nvd_api_key_here

# Optional
GITHUB_TOKEN=your_github_pat_here
```

### API Key Setup

1. **NVD API Key** (Required)
   - Register at: https://nvd.nist.gov/developers/request-an-api-key
   - Free tier: 5000 requests/day
   - Set: `export NVD_API_KEY=your_key`

2. **GitHub Token** (Optional)
   - Create PAT at: https://github.com/settings/tokens
   - Scope: `security_events` (read-only)
   - Set: `export GITHUB_TOKEN=your_token`

## Error Handling

### NVD API Unavailable
```
Fallback: Use cached CVE data (if available) + manual review flag
Degraded Output: Lower confidence score (0.6-0.7)
```

### Rate Limit Exceeded
```
Strategy: Queue request, exponential backoff
Wait Time: 1h for NVD, 15min for GitHub
Alert: Notify security team if critical CVE
```

### CVE Not Found
```
Action: Search alternative sources (GitHub, vendor advisories)
Output: "CVE not found in NVD. Limited data available."
Confidence: 0.5 (manual review recommended)
```

### Parsing Error
```
Action: Log raw response, attempt manual parsing
Fallback: Return partial data + error flag
Confidence: 0.4 (requires manual verification)
```

## Best Practices

### For Security Teams

1. **Run Daily Scans:** Schedule daily dependency scans
2. **Prioritize Critical:** Focus on CVSS >= 9.0 first
3. **Verify Patches:** Confirm patches before applying
4. **Track Trends:** Monitor risk score changes over time
5. **Manual Review:** Always review Critical/High risks

### For Forecasting

1. **Risk as Signal:** Use risk levels as forecast drivers
2. **Trend Analysis:** Track vulnerability disclosure rates
3. **Vendor Response:** Monitor patch availability timelines
4. **Threat Context:** Consider active exploitation patterns
5. **Confidence Weighting:** Weight evidence by confidence scores

## Limitations

- **NVD Lag:** CVE data may lag 24-48 hours after disclosure
- **False Positives:** Version matching may flag patched dependencies
- **Private Vulnerabilities:** Cannot detect non-public CVEs
- **Configuration Context:** Does not assess deployment-specific mitigations
- **Zero-Days:** Cannot predict unknown vulnerabilities

## Troubleshooting

### Low Confidence Scores (<0.75)

**Possible Causes:**
- CVE data incomplete or conflicting
- Multiple sources disagree on severity
- Limited threat intelligence available

**Action:** Request manual security review

### High Risk, Low Exploitability

**Interpretation:** Theoretical vulnerability with limited real-world risk

**Action:** Monitor for exploitation activity, defer patching if operationally disruptive

### Conflicting Risk Ratings

**Example:** CVSS 9.1 but GitHub rates "Moderate"

**Resolution:** Agent favors CVSS (standardized), notes discrepancy in justification

## Coming Soon

- Real-time threat feed integration
- Automated patch deployment recommendations
- Custom risk scoring models per organization
- Integration with SIEM/SOAR platforms
- Vulnerability trend forecasting

## Support

Questions or issues? Contact the Fermi Security Team or file an issue in the repository.

---

**Last Updated:** 2026-01-20  
**Agent Version:** 1.0.0  
**Ontology Version:** 1.0.0
