# Agent Bestiary API Specification

**Version**: 1.0.0  
**Base URL**: `https://your-deployment.vercel.app/api`  
**Authentication**: Bearer token (API key)

---

## Overview

The Agent Bestiary API provides RESTful access to agent memory, ontologies, and knowledge graphs. This API is designed for:

- Agent frameworks to store and retrieve memories
- Developers building AI applications
- Analytics and monitoring tools
- GDPR compliance workflows

---

## Authentication

All API requests require authentication via Bearer token:

```http
Authorization: Bearer <your-api-key>
```

API keys are scoped to organizations and can have read-only or read-write permissions.

---

## Endpoints

### Agents

#### List all agents

```http
GET /api/agents
```

**Response:**
```json
{
  "agents": [
    {
      "agent_id": "uuid",
      "agent_name": "market_research",
      "agent_type": "forecaster",
      "version": "1.0.0",
      "tier": "curated",
      "created_at": "2026-02-07T12:00:00Z",
      "github_url": "https://github.com/Fermi-Agents/market_research",
      "stats": {
        "total_episodes": 1250,
        "total_rules": 45,
        "total_entities": 120,
        "last_consolidated_at": "2026-02-07T02:00:00Z"
      }
    }
  ],
  "total": 10,
  "page": 1,
  "per_page": 20
}
```

**Query Parameters:**
- `page` (optional): Page number (default: 1)
- `per_page` (optional): Items per page (default: 20, max: 100)
- `type` (optional): Filter by agent type
- `tier` (optional): Filter by tier (curated/community)

---

#### Get agent details

```http
GET /api/agents/{agent_id}
```

**Response:**
```json
{
  "agent_id": "uuid",
  "agent_name": "market_research",
  "agent_type": "forecaster",
  "version": "1.0.0",
  "tier": "curated",
  "executor_type": "llm",
  "model": "claude-sonnet-4-5",
  "temperature": 0.3,
  "description": "Market research and TAM analysis agent",
  "author": "Fermi Team",
  "created_at": "2026-02-07T12:00:00Z",
  "github_url": "https://github.com/Fermi-Agents/market_research",
  "current_ontology": {
    "commit_sha": "abc123...",
    "snapshot_id": "uuid",
    "created_at": "2026-02-07T02:00:00Z"
  },
  "stats": {
    "total_episodes": 1250,
    "unconsolidated_episodes": 50,
    "total_rules": 45,
    "verified_rules": 42,
    "total_entities": 120,
    "current_entities": 118,
    "total_facts": 250,
    "current_facts": 248,
    "last_consolidated_at": "2026-02-07T02:00:00Z"
  },
  "performance": {
    "total_executions": 1500,
    "successful_executions": 1475,
    "failed_executions": 25,
    "avg_execution_time_ms": 2500,
    "total_cost_usd": 15.50
  }
}
```

---

#### Create agent

```http
POST /api/agents
```

**Request Body:**
```json
{
  "agent_name": "my_agent",
  "agent_type": "forecaster",
  "executor_type": "llm",
  "model": "claude-sonnet-4-5",
  "temperature": 0.3,
  "description": "My custom agent",
  "author": "Your Name"
}
```

**Response:**
```json
{
  "agent_id": "uuid",
  "agent_name": "my_agent",
  "created_at": "2026-02-07T12:00:00Z",
  "github_url": null
}
```

---

#### Delete agent (GDPR)

```http
DELETE /api/agents/{agent_id}
```

**Query Parameters:**
- `delete_github_repo` (optional): Also delete GitHub repository (default: false)

**Response:**
```json
{
  "deleted": true,
  "agent_id": "uuid",
  "github_repo_deleted": true,
  "deleted_at": "2026-02-07T12:00:00Z"
}
```

---

### Episodes

#### Store episode

```http
POST /api/agents/{agent_id}/episodes
```

**Request Body:**
```json
{
  "query": "What is AMD's current market share in datacenter GPUs?",
  "context": {
    "driver": "market_share",
    "previous_estimate": 0.15
  },
  "execution_status": "success",
  "execution_time_ms": 2500,
  "tokens_used": 1500,
  "cost_usd": 0.015,
  "timestamp": "2026-02-07T12:00:00Z"
}
```

**Response:**
```json
{
  "episode_id": "uuid",
  "agent_id": "uuid",
  "created_at": "2026-02-07T12:00:00Z",
  "embedded": true
}
```

---

#### List episodes

```http
GET /api/agents/{agent_id}/episodes
```

**Query Parameters:**
- `page`, `per_page`: Pagination
- `status`: Filter by status (success/failure/partial)
- `consolidated`: Filter by consolidation status (true/false)
- `since`: ISO timestamp for recent episodes

**Response:**
```json
{
  "episodes": [
    {
      "episode_id": "uuid",
      "query": "What is...",
      "execution_status": "success",
      "timestamp": "2026-02-07T12:00:00Z",
      "consolidated": false
    }
  ],
  "total": 1250,
  "page": 1,
  "per_page": 20
}
```

---

#### Search similar episodes

```http
POST /api/agents/{agent_id}/episodes/search
```

**Request Body:**
```json
{
  "query": "AMD market share",
  "limit": 10,
  "threshold": 0.7
}
```

**Response:**
```json
{
  "results": [
    {
      "episode_id": "uuid",
      "query": "What is AMD's current market share?",
      "similarity": 0.95,
      "timestamp": "2026-02-06T15:30:00Z"
    }
  ]
}
```

---

### Semantic Rules

#### Get agent rules

```http
GET /api/agents/{agent_id}/rules
```

**Query Parameters:**
- `status`: Filter by verification status (verified/pending/rejected)
- `min_confidence`: Minimum confidence score (0.0-1.0)

**Response:**
```json
{
  "rules": [
    {
      "rule_id": "uuid",
      "rule_content": "AMD forecasts require semiconductor industry data",
      "confidence_score": 0.85,
      "verification_status": "verified",
      "created_at": "2026-02-05T02:00:00Z",
      "source_episode_count": 15,
      "application_count": 8,
      "successful_applications": 7
    }
  ],
  "total": 45
}
```

---

### Knowledge Graph

#### Get ontology

```http
GET /api/agents/{agent_id}/ontology
```

**Query Parameters:**
- `format`: Output format (json/mermaid)
- `version`: Snapshot version (default: latest)

**Response (JSON):**
```json
{
  "snapshot_id": "uuid",
  "agent_id": "uuid",
  "version": 5,
  "git_commit_sha": "abc123...",
  "github_url": "https://github.com/Fermi-Agents/market_research",
  "created_at": "2026-02-07T02:00:00Z",
  "stats": {
    "entity_count": 120,
    "fact_count": 250,
    "rule_count": 45
  },
  "entities": [
    {
      "entity_id": "uuid",
      "entity_name": "AMD",
      "entity_type": "company",
      "summary": "Semiconductor company specializing in CPUs and GPUs"
    }
  ],
  "facts": [
    {
      "fact_id": "uuid",
      "source_entity": "AMD",
      "target_entity": "GPU_Market",
      "relation_type": "COMPETES_IN",
      "cardinality": "many_to_one",
      "confidence": 0.95
    }
  ]
}
```

**Response (Mermaid):**
```json
{
  "snapshot_id": "uuid",
  "mermaid_content": "erDiagram\n    AMD ||--o{ GPU : produces\n    AMD }o--|| GPU_Market : competes_in\n"
}
```

---

#### Get entities

```http
GET /api/agents/{agent_id}/entities
```

**Query Parameters:**
- `type`: Filter by entity type
- `search`: Search entity names
- `current_only`: Only current versions (default: true)

**Response:**
```json
{
  "entities": [
    {
      "entity_id": "uuid",
      "entity_name": "AMD",
      "entity_type": "company",
      "summary": "...",
      "valid_from": "2026-01-01T00:00:00Z",
      "valid_to": null,
      "version": 3
    }
  ],
  "total": 120
}
```

---

#### Get entity relationships

```http
GET /api/agents/{agent_id}/entities/{entity_id}/relationships
```

**Response:**
```json
{
  "entity": {
    "entity_id": "uuid",
    "entity_name": "AMD"
  },
  "relationships": [
    {
      "fact_id": "uuid",
      "direction": "outbound",
      "relation_type": "COMPETES_IN",
      "target_entity": {
        "entity_id": "uuid",
        "entity_name": "GPU_Market"
      },
      "confidence": 0.95
    }
  ]
}
```

---

### Consolidation

#### Trigger consolidation

```http
POST /api/agents/{agent_id}/consolidate
```

**Request Body (optional):**
```json
{
  "epsilon": 0.3,
  "min_samples": 2
}
```

**Response:**
```json
{
  "job_id": "uuid",
  "status": "queued",
  "queued_at": "2026-02-07T12:00:00Z"
}
```

---

#### Get consolidation job status

```http
GET /api/consolidation/jobs/{job_id}
```

**Response:**
```json
{
  "job_id": "uuid",
  "agent_id": "uuid",
  "status": "completed",
  "started_at": "2026-02-07T02:00:00Z",
  "completed_at": "2026-02-07T02:05:30Z",
  "duration_ms": 330000,
  "results": {
    "episodes_processed": 50,
    "clusters_identified": 8,
    "rules_extracted": 5,
    "rules_verified": 4,
    "entities_created": 12,
    "facts_created": 25,
    "snapshot_id": "uuid"
  }
}
```

---

### Statistics

#### Get agent statistics

```http
GET /api/agents/{agent_id}/stats
```

**Query Parameters:**
- `period`: Time period (day/week/month/year/all)

**Response:**
```json
{
  "agent_id": "uuid",
  "period": "month",
  "memory": {
    "total_episodes": 1250,
    "episodes_this_period": 250,
    "total_rules": 45,
    "rules_this_period": 8,
    "total_entities": 120,
    "entities_this_period": 15
  },
  "consolidation": {
    "last_consolidation": "2026-02-07T02:00:00Z",
    "consolidations_this_period": 30,
    "avg_duration_ms": 330000
  },
  "performance": {
    "total_executions": 1500,
    "executions_this_period": 300,
    "success_rate": 0.983,
    "avg_execution_time_ms": 2500,
    "total_cost_usd": 15.50
  },
  "ontology": {
    "current_version": 5,
    "versions_this_period": 2,
    "github_url": "https://github.com/Fermi-Agents/market_research"
  }
}
```

---

## Error Responses

All errors follow this format:

```json
{
  "error": {
    "code": "AGENT_NOT_FOUND",
    "message": "Agent with ID 'uuid' not found",
    "status": 404
  }
}
```

### Error Codes

| Code | Status | Description |
|------|--------|-------------|
| `UNAUTHORIZED` | 401 | Invalid or missing API key |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `AGENT_NOT_FOUND` | 404 | Agent does not exist |
| `EPISODE_NOT_FOUND` | 404 | Episode does not exist |
| `VALIDATION_ERROR` | 400 | Invalid request data |
| `RATE_LIMIT_EXCEEDED` | 429 | Too many requests |
| `INTERNAL_ERROR` | 500 | Server error |
| `DATABASE_ERROR` | 503 | Database unavailable |

---

## Rate Limits

- **Free tier**: 100 requests/hour
- **Pro tier**: 1000 requests/hour
- **Enterprise**: Custom limits

Rate limit headers:
```http
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1675785600
```

---

## Webhooks (Future)

Subscribe to events:

```http
POST /api/webhooks
```

**Events:**
- `agent.created`
- `episode.stored`
- `consolidation.started`
- `consolidation.completed`
- `ontology.updated`

---

## SDKs (Future)

- **Python**: `pip install agent-bestiary`
- **Node.js**: `npm install agent-bestiary`
- **Rust**: `cargo add agent-bestiary-client`

---

## Examples

### Store and consolidate workflow

```python
import requests

BASE_URL = "https://your-deployment.vercel.app/api"
API_KEY = "your-api-key"
headers = {"Authorization": f"Bearer {API_KEY}"}

# 1. Create agent
agent = requests.post(
    f"{BASE_URL}/agents",
    headers=headers,
    json={
        "agent_name": "my_forecaster",
        "agent_type": "forecaster",
        "executor_type": "llm",
        "model": "claude-sonnet-4-5"
    }
).json()

agent_id = agent["agent_id"]

# 2. Store episodes
for i in range(10):
    requests.post(
        f"{BASE_URL}/agents/{agent_id}/episodes",
        headers=headers,
        json={
            "query": f"Query {i}",
            "execution_status": "success",
            "execution_time_ms": 2000
        }
    )

# 3. Trigger consolidation
job = requests.post(
    f"{BASE_URL}/agents/{agent_id}/consolidate",
    headers=headers
).json()

# 4. Check status
status = requests.get(
    f"{BASE_URL}/consolidation/jobs/{job['job_id']}",
    headers=headers
).json()

print(f"Status: {status['status']}")

# 5. Get ontology
ontology = requests.get(
    f"{BASE_URL}/agents/{agent_id}/ontology?format=json",
    headers=headers
).json()

print(f"Entities: {len(ontology['entities'])}")
```

---

## Changelog

### v1.0.0 (2026-02-07)
- Initial API release
- Agent management endpoints
- Episode storage and search
- Knowledge graph queries
- Consolidation triggers

---

**Need help?** Check the [Agent Bestiary documentation](./AGENT_BESTIARY_FEATURES.md) or open an issue on GitHub.
