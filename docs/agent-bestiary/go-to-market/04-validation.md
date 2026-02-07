# Phase 4: Validation (Weeks 8-12)

**Goal**: Determine if we have product-market fit

## What is Product-Market Fit?

Product-market fit means:
- Users LOVE the product (not just "like" it)
- Organic growth starts happening
- Users would be VERY disappointed if it went away
- Clear use cases emerge
- Revenue grows month-over-month

## Key Metrics to Track

### 1. User Love (Qualitative)

#### Sean Ellis Test
Ask users: "How would you feel if you could no longer use Agent Bestiary?"

- **Very disappointed**: >40% = strong PMF
- **Somewhat disappointed**: Moderate PMF
- **Not disappointed**: No PMF yet

**Target**: >40% say "very disappointed"

#### Net Promoter Score (NPS)
Ask users: "How likely are you to recommend Agent Bestiary to a colleague?" (0-10)

- **Promoters (9-10)**: Love it, will recommend
- **Passives (7-8)**: Satisfied but not enthusiastic
- **Detractors (0-6)**: Unhappy or indifferent

**NPS = % Promoters - % Detractors**

**Target**: NPS > 40 (excellent for B2B SaaS)

### 2. Activation Rate

**Definition**: % of signups that successfully integrate and use the product

**Calculation**: (Users who stored 10+ episodes) / (Total signups)

**Targets**:
- **Strong**: >50% activation
- **Good**: 40-50%
- **Needs work**: <40%

**Current Status**: Track weekly, identify drop-off points

### 3. Retention Cohorts

Track monthly cohorts to see if users stick around:

| Cohort | Month 0 | Month 1 | Month 2 | Month 3 |
|--------|---------|---------|---------|---------|
| Jan    | 100%    | 60%     | ?       | ?       |
| Feb    | 100%    | ?       | ?       | ?       |

**Targets**:
- **Strong**: >60% M1 retention
- **Good**: 50-60%
- **Needs work**: <50%

### 4. Revenue Growth

Track MRR (Monthly Recurring Revenue) growth:

**Week 8**: $200 MRR (baseline)  
**Week 10**: $300 MRR (+50%)  
**Week 12**: $400 MRR (+33%)  

**Targets**:
- **Strong**: 30%+ month-over-month growth
- **Good**: 20-30%
- **Needs work**: <20%

### 5. Organic Growth

Signs of organic growth:
- Users inviting teammates (team accounts)
- Mentions on Twitter without prompting
- Questions about Agent Bestiary on Reddit/HN
- GitHub stars increasing steadily
- Inbound demo requests

**Target**: 20%+ of signups come from referrals/organic by Week 12

## Customer Interview Protocol

Interview 10-15 users during Weeks 8-12.

### Who to Interview
- 5 power users (using daily)
- 3 paying customers
- 2 churned users (stopped using)
- 3 free tier users considering upgrade
- 2 users who signed up but never activated

### Interview Structure (30 minutes)

**Part 1: Discovery (10 min)**
1. Tell me about what you're building
2. What made you look for a memory solution?
3. What were you using before Agent Bestiary?
4. How did you find us?

**Part 2: Experience (10 min)**
5. Walk me through your first time using Agent Bestiary
6. What clicked for you? What was confusing?
7. How often do you use it now?
8. What's the most valuable feature?
9. What's missing or frustrating?

**Part 3: Value (10 min)**
10. How would you feel if Agent Bestiary disappeared tomorrow?
11. How likely are you to recommend it? (0-10) Why?
12. Is the pricing fair? Too cheap? Too expensive?
13. What would make you upgrade to Pro? (if free tier)
14. What would make you cancel? (if paying)

### Red Flags to Listen For
- "I'm just trying it out" (not solving real problem)
- "It's pretty good" (lukewarm, not love)
- "I wish it did X instead" (positioning mismatch)
- "Too complicated to set up" (onboarding failure)
- "Not sure if I'll keep using it" (weak retention signal)

### Green Flags to Listen For
- "This solves exactly my problem"
- "Way better than what I was using"
- "I told my whole team about this"
- "Can't imagine going back"
- "Worth every penny"

## Data Analysis Dashboard

Build simple dashboard to track:

### User Metrics
- Total signups (cumulative)
- Weekly signups (trend)
- Activation rate (%)
- Weekly active users (count)
- Monthly active users (count)
- WAU/MAU ratio (stickiness)

### Revenue Metrics
- MRR (monthly recurring revenue)
- ARPU (average revenue per user)
- Conversion rate (free → paid)
- Churn rate (%)
- LTV (lifetime value estimate)

### Product Usage
- Episodes stored per user (avg)
- Consolidations run per user (avg)
- API calls per day (trend)
- Errors per day (trend)
- Most used endpoints

### Acquisition
- Signups by source (HN, Reddit, organic, referral)
- Conversion rate by source
- Cost per acquisition (if running ads)

**Tool recommendation**: PostHog (free tier) or Mixpanel

## Product-Market Fit Scorecard

Rate each dimension 1-5 (5 = strong PMF):

| Dimension | Score | Evidence |
|-----------|-------|----------|
| User love | ?/5 | Sean Ellis test, NPS |
| Activation | ?/5 | % users who integrate successfully |
| Retention | ?/5 | Month-over-month cohort retention |
| Revenue growth | ?/5 | MRR growth rate |
| Organic growth | ?/5 | % signups from referrals |
| Use case clarity | ?/5 | Can users articulate value? |
| Word of mouth | ?/5 | Users recommending unprompted |
| **Total** | **?/35** | **Average score** |

**Interpretation**:
- **30-35 points**: Strong PMF, focus on scaling
- **20-29 points**: Moderate PMF, iterate on positioning/product
- **<20 points**: No PMF yet, major pivot or perseverance needed

## Product-Market Fit Hypotheses

Test these hypotheses during validation:

### Hypothesis 1: Target Customer
**Initial belief**: AI agent framework developers  
**Test**: Are most paying customers framework developers?  
**If no**: Who are they? Pivot to that segment.

### Hypothesis 2: Core Value Prop
**Initial belief**: "Episodic → semantic consolidation"  
**Test**: Do users describe value this way?  
**If no**: How do they describe it? Update messaging.

### Hypothesis 3: Primary Use Case
**Initial belief**: Production AI agents in startups  
**Test**: What are users actually building?  
**If different**: Lean into actual use case.

### Hypothesis 4: Pricing
**Initial belief**: $20/month is fair for Pro  
**Test**: Are free users converting at 15%+?  
**If no**: Is price too high? Or is free tier too generous?

### Hypothesis 5: Key Feature
**Initial belief**: Consolidation is killer feature  
**Test**: Do users cite consolidation as most valuable?  
**If no**: What feature do they value most? Double down.

## Three Scenarios

### Scenario 1: Strong PMF (Total score 30+)

**Evidence**:
- >40% users "very disappointed" if product disappeared
- 60%+ M1 retention
- 30%+ MRR growth
- Users recommending unprompted
- Clear use cases emerging

**Action**:
- Double down on what's working
- Prepare to scale to 1000+ users
- Consider fundraising if needed
- Hire first team member (engineering or sales)
- Invest in growth channels

**Focus**: Scaling, not pivoting

### Scenario 2: Moderate PMF (Score 20-29)

**Evidence**:
- Some users love it, others lukewarm
- 40-60% M1 retention
- 15-25% MRR growth
- Mixed feedback on value prop
- Use cases somewhat unclear

**Action**:
- Identify which user segment loves it most
- Pivot positioning to focus on that segment
- Improve onboarding for that use case
- Cut features that don't serve core use case
- Re-launch with sharper positioning

**Focus**: Iteration, not scaling yet

### Scenario 3: No PMF Yet (Score <20)

**Evidence**:
- <30% users "very disappointed"
- <40% M1 retention
- <15% MRR growth
- Users confused about value
- No organic growth

**Action**:
- Deep customer interviews (20+ users)
- Identify if problem is positioning or product
- Consider major pivot or persevere decision
- Test new positioning with new landing page
- Possibly shut down and return to drawing board

**Focus**: Finding PMF, not growth

## Week-by-Week Validation Plan

### Week 8
- [ ] Send Sean Ellis survey to all users
- [ ] Interview 3 power users
- [ ] Set up analytics dashboard
- [ ] Document top 10 feature requests

### Week 9
- [ ] Interview 3 paying customers
- [ ] Calculate M1 retention for January cohort
- [ ] Analyze activation funnel
- [ ] Test 1 hypothesis (e.g., pricing)

### Week 10
- [ ] Interview 2 churned users
- [ ] Interview 2 free tier users
- [ ] Calculate NPS score
- [ ] Review MRR growth

### Week 11
- [ ] Interview 3 recently activated users
- [ ] Complete PMF scorecard
- [ ] Synthesize all interview insights
- [ ] Draft PMF assessment report

### Week 12
- [ ] Review all validation data
- [ ] Make PMF determination (strong/moderate/none)
- [ ] Decide on next phase strategy
- [ ] Share findings with stakeholders
- [ ] Plan Q2 roadmap based on learnings

## Common Validation Mistakes

❌ **Relying only on vanity metrics** (signups, page views)  
✅ **Focus on retention and revenue**

❌ **Ignoring churned users** (survivorship bias)  
✅ **Interview churned users to learn why**

❌ **Cherry-picking positive feedback**  
✅ **Seek out critics and understand objections**

❌ **Declaring PMF too early** (feels good to celebrate)  
✅ **Be ruthlessly honest about metrics**

❌ **Analysis paralysis** (collecting data forever)  
✅ **Make decision by Week 12 and move forward**

## Decision Framework

By end of Week 12, make ONE of these decisions:

### Decision 1: Scale (Strong PMF)
- Invest in growth
- Hire team members
- Raise funding if needed
- Focus on distribution

### Decision 2: Iterate (Moderate PMF)
- Pivot positioning
- Focus on best segment
- Improve core experience
- Re-launch in 4-6 weeks

### Decision 3: Pivot (No PMF)
- Major product changes OR
- Different target customer OR
- Return to research phase

### Decision 4: Shut Down
- If no path to PMF visible
- If market too small
- If competition too strong
- If founder not passionate anymore

## Next Phase

After validation, proceed to [Iteration (Ongoing)](05-iteration.md) with strategy based on PMF determination.
