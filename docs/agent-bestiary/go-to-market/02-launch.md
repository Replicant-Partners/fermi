# Phase 2: Launch (Week 5)

**Goal**: Generate awareness and first wave of signups

## Launch Day Sequence

### Morning (8-10 AM PT)
1. **Publish blog posts** on company blog
2. **Post to Hacker News** with title: "Agent Bestiary – Real Memory for AI Agents"
3. **Tweet launch thread** (10-12 tweets)
4. **Post to Reddit** r/MachineLearning, r/LocalLLaMA, r/LangChain

### Afternoon (12-3 PM PT)
5. **Engage with HN comments** - be responsive, helpful, technical
6. **Share to AI Discord servers** (LangChain, AutoGPT, AI Engineer)
7. **Email beta testers** asking them to try and upvote

### Evening (5-7 PM PT)
8. **Respond to all feedback** - Twitter, HN, Reddit, Discord
9. **Monitor signups and errors** - fix critical issues immediately
10. **Post launch recap** - "Thanks for the support! Here's what we learned..."

## Hacker News Strategy

### Post Title Options (Test with beta testers)
- "Agent Bestiary – Real Memory for AI Agents"
- "Show HN: GDPR-compliant memory for AI agents"
- "Active Dreaming Memory: Episodic → Semantic consolidation for AI agents"

### Post Format
```
Agent Bestiary – Real Memory for AI Agents

Hi HN! I built Agent Bestiary to solve a problem I kept hitting: 
AI agents that forget everything between sessions.

The key insight: agents need TWO types of memory, just like humans:
- Episodic memory: "What happened?" (stored in PostgreSQL)
- Semantic memory: "What did I learn?" (extracted via consolidation)

What makes it different:
- GitHub as source of truth (transparent, auditable learning)
- GDPR-compliant by design (per-agent repos = easy deletion)
- Works with any framework (LangChain, AutoGPT, custom)
- Production-ready (PostgreSQL + pgvector)

Demo: https://agent-bestiary.world/demo
Docs: https://agent-bestiary.world/docs
Try it: https://agent-bestiary.world

Happy to answer questions about the architecture, GDPR compliance, 
or how consolidation works under the hood!
```

### Engagement Strategy
- Respond to every comment within 1 hour
- Be technical and honest (HN appreciates depth)
- Share code snippets if people ask
- Admit limitations and future roadmap
- Thank people for upvoting and trying it

### Success Metrics
- 300+ upvotes (good launch)
- 500+ upvotes (great launch)
- Front page for 6+ hours
- 100+ signups from HN

## Reddit Strategy

### r/MachineLearning Post
**Title**: "Agent Bestiary: Memory system for AI agents with episodic→semantic consolidation"

**Content**:
```
Built a memory backend for AI agents that mimics human memory consolidation.

The problem: Most agents store everything in vector DBs but never extract 
higher-level patterns. They have "perfect recall" but no real learning.

The solution: Two-stage memory like humans:
1. Episodes stored in detail (PostgreSQL + pgvector)
2. Consolidation extracts semantic rules (via LLM reflection)
3. Agents can reason over both episode details AND learned patterns

Technical details:
- PostgreSQL + pgvector for hybrid search
- Per-agent git repos for GDPR compliance
- Multi-provider embeddings (Anthropic, OpenAI, Mistral)
- REST API (Vercel + Rust)

Open to feedback on architecture! Particularly interested in:
- Better consolidation prompts
- Alternatives to git for ontology storage
- Integration patterns with popular frameworks

[Links to demo, docs, GitHub]
```

### r/LocalLLaMA Post
**Title**: "Memory system for local AI agents - no cloud required"

**Focus on**:
- Self-hosting support
- Local LLM compatibility
- Privacy-first design
- No vendor lock-in

### r/LangChain Post
**Title**: "Integration library for LangChain agents with semantic memory"

**Focus on**:
- Easy LangChain integration
- Drop-in memory replacement
- Code examples

### Engagement Strategy
- Post during peak hours (2-4 PM ET)
- Engage with comments quickly
- Cross-post to relevant subreddits
- Don't spam - max 3 subreddits

## Twitter/X Strategy

### Launch Thread (10-12 tweets)

```
1/ 🧠 Launching Agent Bestiary today!

Real memory for AI agents - episodic to semantic consolidation that actually works.

Thread on why agents need TWO types of memory (and how we built it):

2/ The problem: Current agents have "perfect recall" but no real learning.

They dump everything into vector DBs, then search for similar stuff.

But humans don't work like that...

3/ Humans have TWO memory systems:

📖 Episodic: "What happened?" (detailed, short-term)
📚 Semantic: "What did I learn?" (patterns, long-term)

We sleep to consolidate episodes → semantic knowledge.

4/ Agents need the same thing!

Agent Bestiary gives agents:
- Episodes stored with full context
- Consolidation that extracts patterns
- Semantic rules they can reason over

5/ Example:

Episode: "User asked about pricing, I quoted $20/mo, they said too expensive"

After consolidation:

Semantic rule: "Users often find $20/mo expensive. Suggest yearly discount first."

6/ Technical approach:

📊 PostgreSQL + pgvector (hybrid search)
🔄 Consolidation via LLM reflection
📁 Git repos for transparent learning
🔐 GDPR-compliant by design

7/ Why git for ontologies?

- Transparent: users can see what their agent learned
- Auditable: full history of knowledge evolution
- Portable: clone your agent's knowledge anytime
- GDPR-friendly: delete repo = delete agent

8/ GDPR compliance is a first-class feature:

✅ Right to access (clone repo)
✅ Right to erasure (delete repo)
✅ Right to portability (git format)
✅ Right to rectification (pull requests!)

9/ Works with any framework:

- LangChain ✅
- AutoGPT ✅
- CrewAI ✅
- Custom agents ✅

Drop-in memory replacement via REST API.

10/ Pricing:

Free: 1 agent, 100 episodes
Pro: $20/mo, unlimited agents
Enterprise: Custom

Start free: [link]

11/ What's next:

- Multi-agent shared memory
- Knowledge graph visualization
- Consolidation scheduling
- More framework integrations

12/ Thanks to beta testers who gave early feedback! 🙏

Try it: [link]
Docs: [link]
Demo: [link]

Questions? Reply below! 👇
```

### Hashtag Strategy
- #AIAgents #LangChain #MachineLearning #AI
- Tag relevant accounts: @LangChainAI, @AutoGPT, etc.

### Engagement
- Respond to all replies within 2 hours
- Quote tweet positive feedback
- Share screenshots of people trying it

## Discord/Slack Communities

### Communities to Post In
1. LangChain Discord (#show-and-tell)
2. AutoGPT Discord (#general)
3. AI Engineer Discord (#projects)
4. Latent Space Discord (#side-projects)

### Message Template
```
👋 Hey everyone! Just launched Agent Bestiary - a memory backend for AI agents.

The key idea: agents need episodic + semantic memory, just like humans. 
We built consolidation that extracts patterns from episodes.

GDPR-compliant by design (per-agent git repos).

Would love feedback from this community! Free tier available.

[Links]
```

## Email to Beta Testers

**Subject**: "🚀 Agent Bestiary is live!"

**Body**:
```
Hi [Name],

We're launching Agent Bestiary on Hacker News today!

As a beta tester, you've been instrumental in shaping the product. 
Thank you for your early feedback.

If you're willing to support the launch:
- Upvote on HN: [link]
- Share on Twitter: [link]
- Comment with your experience

No pressure - just grateful for your help so far!

[Founder name]
```

## Success Metrics

### Day 1 Goals
- 500+ HN upvotes
- 100+ signups
- 50+ Twitter likes
- Front page of HN for 6+ hours

### Week 5 Goals
- 200 total signups
- 50 active users (created agent, stored episodes)
- 5-10 pieces of feedback/feature requests
- 3-5 integration questions (shows serious interest)

## What "Good" Looks Like

A good launch has:
- Front page HN for most of the day
- Thoughtful technical discussion in comments
- Other people defending your product in replies
- Signups continue for 2-3 days after launch
- 1-2 influencers sharing on Twitter

## Common Launch Mistakes to Avoid

❌ **Disappearing after posting** - be extremely responsive  
❌ **Getting defensive** - take criticism gracefully  
❌ **Over-promising** - be honest about limitations  
❌ **Ignoring negative feedback** - engage with critics  
❌ **Posting too early/late** - aim for 8-10 AM PT  

## Crisis Management

### If HN Post Gets Flagged
- Ask beta testers to vouch in comments
- Reach out to HN mods with context
- Don't create sockpuppets (will get caught)

### If Site Goes Down
- Post status update immediately
- Fix ASAP (have someone on-call)
- Give people Pro tier credit as apology

### If Criticism Is Valid
- Acknowledge it publicly
- Add to roadmap
- Follow up when fixed

## Post-Launch Debrief

After launch day, review:
- What worked well?
- What didn't work?
- Unexpected feedback?
- Critical bugs found?
- Feature requests to prioritize?

Document learnings for next product launch.

## Next Phase

After launch week, proceed to [First 100 Users (Weeks 6-12)](03-first-100-users.md).
