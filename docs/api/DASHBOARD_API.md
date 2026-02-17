# Dashboard API Documentation

**Version:** 1.0.0  
**Base URL:** `https://agent-bestiary.world`  
**Release Date:** 2026-02-17  
**Status:** Production Ready

---

## Overview

The Dashboard API provides spatial queries for situational awareness of creatures and rabbles. It supports:

- **Spatial presence detection** (in area vs outside)
- **Boundary violation warnings**
- **Multi-creature rabble membership**
- **Real-time deployment status**

---

## Authentication

All endpoints require authentication via session cookie or API key.

**Headers:**
```
Cookie: abw_session=<token>
```
OR
```
Authorization: Bearer <api_key>
```

---

## Endpoints

### 1. GET /api/dashboard/my-rabbles

Returns rabbles where the authenticated user has creatures, with distance from center and area status.

**Request:**
```http
GET /api/dashboard/my-rabbles
```

**Response:**
```json
{
  "rabbles": [
    {
      "swarm_id": "uuid",
      "name": "Garden Party at Hyde Park",
      "location_name": "Hyde Park, London",
      "center_lat": 51.5073,
      "center_lng": -0.1657,
      "radius_meters": 100,
      "creature_count": 8,
      "participant_count": 5,
      "starts_at": "2026-02-17T14:00:00Z",
      "ends_at": "2026-02-17T16:00:00Z",
      "status": "active",
      "anchor_creature_id": "uuid",
      "anchor_creature_name": "Luna",
      "anchor_creature_image": "https://...",
      "my_creatures": [
        {
          "creature_id": "uuid",
          "specimen_name": "Luna",
          "scientific_name": "Actias luna",
          "location_lat": 51.5075,
          "location_lng": -0.1655,
          "distance_meters": 22.5,
          "in_area": true
        },
        {
          "creature_id": "uuid",
          "specimen_name": "Atlas",
          "scientific_name": "Attacus atlas",
          "location_lat": 51.5085,
          "location_lng": -0.1670,
          "distance_meters": 120.3,
          "in_area": false
        }
      ]
    }
  ]
}
```

**Status Codes:**
- `200 OK` - Success
- `401 Unauthorized` - Authentication required
- `500 Internal Server Error` - Database error

**Use Case:** Dashboard "My Rabbles" tab

---

### 2. GET /api/dashboard/nearby

Returns rabbles near a specific location, with "in area" indicator for observer presence.

**Request:**
```http
GET /api/dashboard/nearby?lat=51.5&lng=-0.1&radius=1000
```

**Query Parameters:**
- `lat` (required): User's latitude
- `lng` (required): User's longitude
- `radius` (optional): Search radius in meters (default: 1000)

**Response:**
```json
{
  "rabbles": [
    {
      "swarm_id": "uuid",
      "name": "Bee Watch at Community Garden",
      "location_name": "Community Garden",
      "center_lat": 51.5001,
      "center_lng": -0.1002,
      "radius_meters": 50,
      "creature_count": 3,
      "participant_count": 2,
      "starts_at": "2026-02-17T10:00:00Z",
      "ends_at": "2026-02-17T18:00:00Z",
      "status": "active",
      "anchor_creature_id": null,
      "anchor_creature_name": null,
      "anchor_creature_image": null,
      "distance_meters": 50.2,
      "user_in_area": true
    },
    {
      "swarm_id": "uuid",
      "name": "Garden Party at Hyde Park",
      "location_name": "Hyde Park, London",
      "center_lat": 51.5073,
      "center_lng": -0.1657,
      "radius_meters": 100,
      "creature_count": 8,
      "participant_count": 5,
      "starts_at": "2026-02-17T14:00:00Z",
      "ends_at": "2026-02-17T16:00:00Z",
      "status": "active",
      "anchor_creature_id": "uuid",
      "anchor_creature_name": "Luna",
      "anchor_creature_image": "https://...",
      "distance_meters": 350.5,
      "user_in_area": false
    }
  ]
}
```

**Status Codes:**
- `200 OK` - Success
- `400 Bad Request` - Missing lat/lng parameters
- `401 Unauthorized` - Authentication required
- `500 Internal Server Error` - Database error

**Use Case:** Dashboard "Nearby" tab

---

### 3. GET /api/dashboard/creatures

Returns user's creatures with deployment status and rabble membership.

**Request:**
```http
GET /api/dashboard/creatures?status=active&limit=200
```

**Query Parameters:**
- `status` (optional): Creature status filter (default: "active")
- `limit` (optional): Maximum results (default: 200)

**Response:**
```json
{
  "creatures": [
    {
      "creature_id": "uuid",
      "specimen_name": "Luna",
      "scientific_name": "Actias luna",
      "species_group": "butterfly",
      "asset_path": "https://...",
      "rabble_id": "uuid",
      "rabble_name": "Garden Party at Hyde Park",
      "location_lat": 51.5075,
      "location_lng": -0.1655,
      "h3_cell": "8c2a1072b59ffff",
      "state": "in_rabble",
      "presence": "active",
      "distance_from_rabble_center": 22.5,
      "in_rabble_area": true
    },
    {
      "creature_id": "uuid",
      "specimen_name": "Atlas",
      "scientific_name": "Attacus atlas",
      "species_group": "butterfly",
      "asset_path": "https://...",
      "rabble_id": null,
      "rabble_name": null,
      "location_lat": 51.5085,
      "location_lng": -0.1670,
      "h3_cell": "8c2a1072b59ffff",
      "state": "perched",
      "presence": "active",
      "distance_from_rabble_center": null,
      "in_rabble_area": null
    }
  ]
}
```

**Status Codes:**
- `200 OK` - Success
- `401 Unauthorized` - Authentication required
- `500 Internal Server Error` - Database error

**Use Case:** Dashboard "Creatures" tab

---

### 4. GET /api/dashboard/boundary-violations

Returns creatures that have left their rabble area (boundary violations).

**Request:**
```http
GET /api/dashboard/boundary-violations
```

**Response:**
```json
{
  "violations": [
    {
      "creature_id": "uuid",
      "specimen_name": "Atlas",
      "rabble_id": "uuid",
      "rabble_name": "Garden Party at Hyde Park",
      "distance_meters": 120.3,
      "rabble_radius": 100
    }
  ]
}
```

**Status Codes:**
- `200 OK` - Success
- `401 Unauthorized` - Authentication required
- `500 Internal Server Error` - Database error

**Use Case:** Boundary warning notifications

---

## Spatial Queries

All spatial calculations use PostgreSQL's PostGIS extension:

- **Distance:** `ST_Distance(geography, geography)` returns meters
- **Area Check:** `ST_DWithin(geography, geography, radius)` returns boolean
- **Coordinates:** `ST_MakePoint(lng, lat)::geography`

---

## Database Functions

The API uses these PostgreSQL functions:

### `get_my_rabbles_with_status(user_id, limit)`
Returns rabbles with creature distances and area status.

### `get_nearby_rabbles(lat, lng, radius, limit)`
Returns nearby rabbles with user-in-area indicator.

### `get_creatures_with_deployment(user_id, status, limit)`
Returns creatures with deployment status.

### `check_boundary_violations(user_id)`
Returns creatures outside their rabble area.

---

## Error Handling

All errors return JSON:

```json
{
  "error": "Error message",
  "code": "ERROR_CODE"
}
```

Common error codes:
- `UNAUTHORIZED` - Authentication required
- `BAD_REQUEST` - Invalid parameters
- `NOT_FOUND` - Resource not found
- `INTERNAL_ERROR` - Server error

---

## Rate Limiting

- **Default:** 300 requests/minute
- **Headers:** `X-RateLimit-Remaining`

---

## Examples

### cURL

```bash
# Get my rabbles
curl -H "Authorization: Bearer YOUR_TOKEN" \
  https://agent-bestiary.world/api/dashboard/my-rabbles

# Get nearby rabbles
curl -H "Authorization: Bearer YOUR_TOKEN" \
  "https://agent-bestiary.world/api/dashboard/nearby?lat=51.5&lng=-0.1"

# Get creatures
curl -H "Authorization: Bearer YOUR_TOKEN" \
  https://agent-bestiary.world/api/dashboard/creatures

# Check boundary violations
curl -H "Authorization: Bearer YOUR_TOKEN" \
  https://agent-bestiary.world/api/dashboard/boundary-violations
```

### JavaScript

```javascript
// Fetch my rabbles
const response = await fetch('/api/dashboard/my-rabbles', {
  headers: {
    'Authorization': `Bearer ${token}`
  }
});
const data = await response.json();
console.log(data.rabbles);
```

---

## Changelog

### v1.0.0 (2026-02-17)
- Initial release
- Four endpoints for dashboard spatial queries
- PostGIS-based distance calculations
- Boundary violation detection

---

## Support

- **Issues:** https://github.com/Replicant-Partners/fermi/issues
- **Docs:** https://agent-bestiary.world/docs
- **Status:** https://status.agent-bestiary.world

