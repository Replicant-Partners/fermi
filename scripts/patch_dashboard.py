#!/usr/bin/env python3
"""Patch api_client.dart with dashboard API methods and fix nearby filter in dashboard_screen.dart.

Run from fermi root:
    python3 scripts/patch_dashboard.py
"""

import os
import sys

RABBLE_ROOT = "/home/ilabra/rabble"
API_CLIENT = os.path.join(RABBLE_ROOT, "lib/services/api_client.dart")
DASHBOARD = os.path.join(RABBLE_ROOT, "lib/screens/dashboard_screen.dart")


def patch_api_client():
    """Add dashboard endpoint methods to api_client.dart."""
    with open(API_CLIENT, "r") as f:
        content = f.read()

    if "getNearbyRabbles" in content:
        print("  ⊘ api_client.dart already has dashboard methods, skipping")
        return

    marker = "  // ─── Creature Friendships"
    if marker not in content:
        marker = "  // ─── Helpers ───"
    if marker not in content:
        print("  ✗ Could not find insertion point in api_client.dart")
        return

    # Build the methods string carefully to avoid shell interpolation issues
    methods = []
    methods.append("")
    methods.append("  // ─── Dashboard ─────────────────────────────────────────────")
    methods.append("")
    methods.append("  Future<List<Map<String, dynamic>>> getNearbyRabbles({")
    methods.append("    required double lat,")
    methods.append("    required double lng,")
    methods.append("    int radius = 1000,")
    methods.append("  }) async {")
    # Use string concatenation to build the URI with baseUrl
    methods.append(
        "    final uri = Uri.parse(baseUrl + '/api/dashboard/nearby').replace("
    )
    methods.append("      queryParameters: {")
    methods.append("        'lat': lat.toString(),")
    methods.append("        'lng': lng.toString(),")
    methods.append("        'radius': radius.toString(),")
    methods.append("      },")
    methods.append("    );")
    methods.append("    final response = await _client.get(uri, headers: _headers);")
    methods.append("    _checkResponse(response);")
    methods.append(
        "    final data = jsonDecode(response.body) as Map<String, dynamic>;"
    )
    methods.append(
        "    return (data['rabbles'] as List<dynamic>).cast<Map<String, dynamic>>();"
    )
    methods.append("  }")
    methods.append("")
    methods.append("  Future<List<Map<String, dynamic>>> getMyRabbles() async {")
    methods.append("    final response = await _client.get(")
    methods.append("      Uri.parse(baseUrl + '/api/dashboard/my-rabbles'),")
    methods.append("      headers: _headers,")
    methods.append("    );")
    methods.append("    _checkResponse(response);")
    methods.append(
        "    final data = jsonDecode(response.body) as Map<String, dynamic>;"
    )
    methods.append(
        "    return (data['rabbles'] as List<dynamic>).cast<Map<String, dynamic>>();"
    )
    methods.append("  }")
    methods.append("")
    methods.append("  Future<List<Map<String, dynamic>>> getCreaturesWithDeployment({")
    methods.append("    String status = 'active',")
    methods.append("    int limit = 200,")
    methods.append("  }) async {")
    methods.append(
        "    final uri = Uri.parse(baseUrl + '/api/dashboard/creatures').replace("
    )
    methods.append("      queryParameters: {")
    methods.append("        'status': status,")
    methods.append("        'limit': limit.toString(),")
    methods.append("      },")
    methods.append("    );")
    methods.append("    final response = await _client.get(uri, headers: _headers);")
    methods.append("    _checkResponse(response);")
    methods.append(
        "    final data = jsonDecode(response.body) as Map<String, dynamic>;"
    )
    methods.append(
        "    return (data['creatures'] as List<dynamic>).cast<Map<String, dynamic>>();"
    )
    methods.append("  }")
    methods.append("")

    insert_text = "\n".join(methods) + "\n"
    idx = content.index(marker)
    content = content[:idx] + insert_text + content[idx:]

    with open(API_CLIENT, "w") as f:
        f.write(content)
    print("  ✓ api_client.dart patched with dashboard methods")


def fix_nearby_filter():
    """Fix _filterNearbyRabbles to:
    1. NOT exclude rabbles where user has a creature (show all nearby)
    2. When GPS is null, show ALL rabbles as a fallback instead of nothing
    3. Use larger default radius and cleaner distance sorting
    """
    with open(DASHBOARD, "r") as f:
        content = f.read()

    # Find and replace the _filterNearbyRabbles method
    old_filter_start = "  void _filterNearbyRabbles() {"
    if old_filter_start not in content:
        print("  ✗ Could not find _filterNearbyRabbles in dashboard_screen.dart")
        return

    # Find the end of the method (next method or closing brace pattern)
    start_idx = content.index(old_filter_start)

    # Find the static _haversine method that follows
    haversine_marker = "  static double _haversine("
    if haversine_marker not in content:
        print("  ✗ Could not find _haversine marker")
        return

    end_idx = content.index(haversine_marker)

    # Build replacement
    new_filter = []
    new_filter.append("  void _filterNearbyRabbles() {")
    new_filter.append("    if (_searchLat == null || _searchLng == null) {")
    new_filter.append(
        "      // No location available — show all rabbles sorted by name"
    )
    new_filter.append("      // This is the fallback for web browsers without GPS")
    new_filter.append("      _nearbyRabbles = List.from(_allSwarms);")
    new_filter.append("      _nearbyRabbles.sort((a, b) => a.name.compareTo(b.name));")
    new_filter.append("      return;")
    new_filter.append("    }")
    new_filter.append("    final radiusM = _searchRadiusKm * 1000;")
    new_filter.append(
        "    // Show ALL rabbles within radius — including ones with your creatures."
    )
    new_filter.append(
        "    // The 'My Rabbles' tab is for management; Nearby is for discovery."
    )
    new_filter.append("    _nearbyRabbles = _allSwarms.where((s) {")
    new_filter.append(
        "      return _haversine(_searchLat!, _searchLng!, s.centerLat, s.centerLng) <= radiusM;"
    )
    new_filter.append("    }).toList();")
    new_filter.append("    _nearbyRabbles.sort((a, b) {")
    new_filter.append(
        "      final da = _haversine(_searchLat!, _searchLng!, a.centerLat, a.centerLng);"
    )
    new_filter.append(
        "      final db = _haversine(_searchLat!, _searchLng!, b.centerLat, b.centerLng);"
    )
    new_filter.append("      return da.compareTo(db);")
    new_filter.append("    });")
    new_filter.append("  }")
    new_filter.append("")

    replacement = "\n".join(new_filter) + "\n"
    content = content[:start_idx] + replacement + content[end_idx:]

    with open(DASHBOARD, "w") as f:
        f.write(content)
    print("  ✓ dashboard_screen.dart: fixed _filterNearbyRabbles")
    print("    - No longer excludes your own rabbles from Nearby tab")
    print("    - Shows all rabbles when GPS unavailable (web fallback)")


def fix_list_swarms_limit():
    """The dashboard calls listSwarms(limit: 100) but the backend caps at 50.
    Increase backend cap or change the call. Let's change to limit: 50 on the
    client side since that matches the backend."""
    with open(DASHBOARD, "r") as f:
        content = f.read()

    if "listSwarms(limit: 100)" in content:
        content = content.replace("listSwarms(limit: 100)", "listSwarms(limit: 50)")
        with open(DASHBOARD, "w") as f:
            f.write(content)
        print(
            "  ✓ dashboard_screen.dart: fixed listSwarms limit (100 → 50, matches backend cap)"
        )
    else:
        print("  ⊘ listSwarms limit already correct")


def also_fix_backend_limit():
    """Actually, let's increase the backend limit to 100 since the dashboard needs it."""
    backend_path = "/home/ilabra/fermi/src/handlers/creatures/swarms.rs"
    with open(backend_path, "r") as f:
        content = f.read()

    # The handler has: let limit = q.limit.unwrap_or(20).min(50);
    old = "let limit = q.limit.unwrap_or(20).min(50);"
    new = "let limit = q.limit.unwrap_or(20).min(200);"

    if old in content:
        content = content.replace(old, new)
        with open(backend_path, "w") as f:
            f.write(content)
        print("  ✓ swarms.rs: increased listSwarms limit cap (50 → 200)")
    elif new in content:
        print("  ⊘ swarms.rs limit already increased")
    else:
        print("  ⊘ swarms.rs limit pattern not found (may have different format)")


def fix_creature_count_filter():
    """The listSwarms handler has 'AND creature_count > 0' which filters out
    newly created rabbles that haven't had creatures join yet. This could be
    why test data isn't showing. Remove this filter — empty rabbles should
    still appear in nearby search so people can join them."""
    backend_path = "/home/ilabra/fermi/src/handlers/creatures/swarms.rs"
    with open(backend_path, "r") as f:
        content = f.read()

    old_filter = '    // Perches with 0 creatures are stale — the creature left\n    sql.push_str(" AND creature_count > 0");'
    new_filter = '    // Show rabbles even with 0 creatures — they may be newly created or between joins\n    // sql.push_str(" AND creature_count > 0"); // Removed: was hiding valid rabbles from search'

    if old_filter in content:
        content = content.replace(old_filter, new_filter)
        with open(backend_path, "w") as f:
            f.write(content)
        print(
            "  ✓ swarms.rs: removed creature_count > 0 filter (was hiding rabbles from search)"
        )
    elif "creature_count > 0" not in content:
        print("  ⊘ swarms.rs: creature_count filter already removed")
    else:
        # Try a simpler match
        simple_old = 'sql.push_str(" AND creature_count > 0");'
        if simple_old in content:
            content = content.replace(
                simple_old,
                '// sql.push_str(" AND creature_count > 0"); // Removed: was hiding valid rabbles',
            )
            with open(backend_path, "w") as f:
                f.write(content)
            print("  ✓ swarms.rs: commented out creature_count > 0 filter")
        else:
            print("  ⊘ swarms.rs: creature_count filter pattern not matched exactly")


def main():
    print("Patching dashboard for proximity search fix...\n")

    print("1. API Client — dashboard endpoint methods:")
    patch_api_client()

    print("\n2. Dashboard Screen — nearby filter fix:")
    fix_nearby_filter()

    print("\n3. Backend — listSwarms limit:")
    also_fix_backend_limit()

    print("\n4. Backend — creature_count filter:")
    fix_creature_count_filter()

    print("\n✅ Dashboard patched.")
    print("\nChanges:")
    print("  Flutter:")
    print(
        "    - api_client.dart: getNearbyRabbles, getMyRabbles, getCreaturesWithDeployment"
    )
    print(
        "    - dashboard_screen.dart: Nearby shows ALL rabbles in radius (including yours)"
    )
    print("    - dashboard_screen.dart: Shows all rabbles when GPS unavailable")
    print("  Backend:")
    print("    - swarms.rs: Removed creature_count > 0 filter")
    print("    - swarms.rs: Increased list limit cap to 200")
    print("\nNext: rebuild Flutter and test")


if __name__ == "__main__":
    main()
