#!/usr/bin/env python3
"""Generate Flutter social layer files for the rabble app.

Run from fermi root:
    python3 scripts/gen_social_flutter.py

Generates:
  - /home/ilabra/rabble/lib/models/creature_friendship.dart
  - /home/ilabra/rabble/lib/screens/rabble_recap.dart
  - /home/ilabra/rabble/lib/widgets/creature_invite_sheet.dart
  - /home/ilabra/rabble/lib/widgets/friendship_request_card.dart
  - /home/ilabra/rabble/lib/widgets/activity_feed.dart
  - Patches api_client.dart with social layer methods
"""

import os
import sys

RABBLE_ROOT = "/home/ilabra/rabble"

# ═══════════════════════════════════════════════════════════════════════════
# 1. MODELS
# ═══════════════════════════════════════════════════════════════════════════

CREATURE_FRIENDSHIP_DART = r"""/// Social layer models: friendships, invites, activity events, recap.
///
/// Friendships are creature-to-creature (symmetric, Layer 2).
/// Invites are "come fly with me" (creature-to-creature, Layer 2).
/// Activity events power the SSE feed with relationship context.

class CreatureFriendship {
  final String friendshipId;
  final String creatureId;
  final String? specimenName;
  final String? speciesGroup;
  final String? assetPath;
  final String? ownerId;
  final String? ownerDisplayName;
  final String? socialVisibility;
  final String? metInRabble;
  final String? rabbleName;
  final DateTime? friendsSince;

  CreatureFriendship({
    required this.friendshipId,
    required this.creatureId,
    this.specimenName,
    this.speciesGroup,
    this.assetPath,
    this.ownerId,
    this.ownerDisplayName,
    this.socialVisibility,
    this.metInRabble,
    this.rabbleName,
    this.friendsSince,
  });

  factory CreatureFriendship.fromJson(Map<String, dynamic> json) {
    return CreatureFriendship(
      friendshipId: json['friendship_id'] as String,
      creatureId: json['creature_id'] as String,
      specimenName: json['specimen_name'] as String?,
      speciesGroup: json['species_group'] as String?,
      assetPath: json['asset_path'] as String?,
      ownerId: json['owner_id'] as String?,
      ownerDisplayName: json['owner_display_name'] as String?,
      socialVisibility: json['social_visibility'] as String?,
      metInRabble: json['met_in_rabble'] as String?,
      rabbleName: json['rabble_name'] as String?,
      friendsSince: json['friends_since'] != null
          ? DateTime.parse(json['friends_since'] as String)
          : null,
    );
  }

  String get displayName => specimenName ?? 'Unknown Creature';
  String get ownerLabel {
    if (socialVisibility == 'private' || socialVisibility == 'creature-only') {
      return 'Anonymous';
    }
    return ownerDisplayName ?? 'Unknown';
  }
}

/// Pending friendship request (inbound)
class FriendshipRequest {
  final String friendshipId;
  final String fromCreatureId;
  final String? fromCreatureName;
  final String? fromSpeciesGroup;
  final String? fromAssetPath;
  final String? fromOwnerId;
  final String? fromOwnerName;
  final String toCreatureId;
  final String? toCreatureName;
  final String? metInRabble;
  final String? rabbleName;
  final DateTime createdAt;

  FriendshipRequest({
    required this.friendshipId,
    required this.fromCreatureId,
    this.fromCreatureName,
    this.fromSpeciesGroup,
    this.fromAssetPath,
    this.fromOwnerId,
    this.fromOwnerName,
    required this.toCreatureId,
    this.toCreatureName,
    this.metInRabble,
    this.rabbleName,
    required this.createdAt,
  });

  factory FriendshipRequest.fromJson(Map<String, dynamic> json) {
    return FriendshipRequest(
      friendshipId: json['friendship_id'] as String,
      fromCreatureId: json['from_creature_id'] as String,
      fromCreatureName: json['from_creature_name'] as String?,
      fromSpeciesGroup: json['from_species_group'] as String?,
      fromAssetPath: json['from_asset_path'] as String?,
      fromOwnerId: json['from_owner_id'] as String?,
      fromOwnerName: json['from_owner_name'] as String?,
      toCreatureId: json['to_creature_id'] as String,
      toCreatureName: json['to_creature_name'] as String?,
      metInRabble: json['met_in_rabble'] as String?,
      rabbleName: json['rabble_name'] as String?,
      createdAt: DateTime.parse(json['created_at'] as String),
    );
  }
}

/// Creature invite ("come fly with me")
class CreatureInvite {
  final String inviteId;
  final String fromCreatureId;
  final String? fromCreatureName;
  final String? fromSpeciesGroup;
  final String? fromAssetPath;
  final String? fromOwnerName;
  final String toCreatureId;
  final String? toCreatureName;
  final String rabbleId;
  final String? rabbleName;
  final String? message;
  final DateTime createdAt;
  final DateTime expiresAt;

  CreatureInvite({
    required this.inviteId,
    required this.fromCreatureId,
    this.fromCreatureName,
    this.fromSpeciesGroup,
    this.fromAssetPath,
    this.fromOwnerName,
    required this.toCreatureId,
    this.toCreatureName,
    required this.rabbleId,
    this.rabbleName,
    this.message,
    required this.createdAt,
    required this.expiresAt,
  });

  factory CreatureInvite.fromJson(Map<String, dynamic> json) {
    return CreatureInvite(
      inviteId: json['invite_id'] as String,
      fromCreatureId: json['from_creature_id'] as String,
      fromCreatureName: json['from_creature_name'] as String?,
      fromSpeciesGroup: json['from_species_group'] as String?,
      fromAssetPath: json['from_asset_path'] as String?,
      fromOwnerName: json['from_owner_name'] as String?,
      toCreatureId: json['to_creature_id'] as String,
      toCreatureName: json['to_creature_name'] as String?,
      rabbleId: json['rabble_id'] as String,
      rabbleName: json['rabble_name'] as String?,
      message: json['message'] as String?,
      createdAt: DateTime.parse(json['created_at'] as String),
      expiresAt: DateTime.parse(json['expires_at'] as String),
    );
  }

  bool get isExpired => DateTime.now().isAfter(expiresAt);
  Duration get timeRemaining => expiresAt.difference(DateTime.now());
}

/// Activity feed event with relationship context
class ActivityEvent {
  final String eventId;
  final String eventType;
  final String actorUserId;
  final String? actorCreatureId;
  final String? actorCreatureName;
  final String? actorSpeciesGroup;
  final String? rabbleId;
  final String? rabbleName;
  final String? targetCreatureId;
  final String? targetCreatureName;
  final String title;
  final String? body;
  final Map<String, dynamic>? metadata;
  final DateTime createdAt;
  final bool isOwnCreature;
  final bool isContact;
  final bool isFriendCreature;

  ActivityEvent({
    required this.eventId,
    required this.eventType,
    required this.actorUserId,
    this.actorCreatureId,
    this.actorCreatureName,
    this.actorSpeciesGroup,
    this.rabbleId,
    this.rabbleName,
    this.targetCreatureId,
    this.targetCreatureName,
    required this.title,
    this.body,
    this.metadata,
    required this.createdAt,
    this.isOwnCreature = false,
    this.isContact = false,
    this.isFriendCreature = false,
  });

  factory ActivityEvent.fromJson(Map<String, dynamic> json) {
    return ActivityEvent(
      eventId: json['event_id'] as String,
      eventType: json['event_type'] as String,
      actorUserId: json['actor_user_id'] as String,
      actorCreatureId: json['actor_creature_id'] as String?,
      actorCreatureName: json['actor_creature_name'] as String?,
      actorSpeciesGroup: json['actor_species_group'] as String?,
      rabbleId: json['rabble_id'] as String?,
      rabbleName: json['rabble_name'] as String?,
      targetCreatureId: json['target_creature_id'] as String?,
      targetCreatureName: json['target_creature_name'] as String?,
      title: json['title'] as String,
      body: json['body'] as String?,
      metadata: json['metadata'] as Map<String, dynamic>?,
      createdAt: DateTime.parse(json['created_at'] as String),
      isOwnCreature: json['is_own_creature'] as bool? ?? false,
      isContact: json['is_contact'] as bool? ?? false,
      isFriendCreature: json['is_friend_creature'] as bool? ?? false,
    );
  }

  /// Priority for visual highlighting: own > friend > contact > other
  int get relationshipPriority {
    if (isOwnCreature) return 3;
    if (isFriendCreature) return 2;
    if (isContact) return 1;
    return 0;
  }

  String get relationshipLabel {
    if (isOwnCreature) return 'Your creature';
    if (isFriendCreature) return 'Friend';
    if (isContact) return 'Contact';
    return '';
  }
}

/// Rabble recap — creature met in a rabble
class RecapCreature {
  final String creatureId;
  final String? specimenName;
  final String? scientificName;
  final String? speciesGroup;
  final String? assetPath;
  final String? ownerId;
  final String? ownerDisplayName;
  final String? ownerSocialVisibility;
  final int? overlapSeconds;
  final bool alreadyFriends;
  final String? friendshipStatus;

  RecapCreature({
    required this.creatureId,
    this.specimenName,
    this.scientificName,
    this.speciesGroup,
    this.assetPath,
    this.ownerId,
    this.ownerDisplayName,
    this.ownerSocialVisibility,
    this.overlapSeconds,
    this.alreadyFriends = false,
    this.friendshipStatus,
  });

  factory RecapCreature.fromJson(Map<String, dynamic> json) {
    return RecapCreature(
      creatureId: json['creature_id'] as String,
      specimenName: json['specimen_name'] as String?,
      scientificName: json['scientific_name'] as String?,
      speciesGroup: json['species_group'] as String?,
      assetPath: json['asset_path'] as String?,
      ownerId: json['owner_id'] as String?,
      ownerDisplayName: json['owner_display_name'] as String?,
      ownerSocialVisibility: json['owner_social_visibility'] as String?,
      overlapSeconds: json['overlap_seconds'] as int?,
      alreadyFriends: json['already_friends'] as bool? ?? false,
      friendshipStatus: json['friendship_status'] as String?,
    );
  }

  String get displayName => specimenName ?? 'Unknown';
  String get ownerLabel {
    if (ownerSocialVisibility == 'private' ||
        ownerSocialVisibility == 'creature-only') {
      return 'Anonymous';
    }
    return ownerDisplayName ?? 'Unknown';
  }

  Duration? get overlapDuration =>
      overlapSeconds != null ? Duration(seconds: overlapSeconds!) : null;
}
"""

# ═══════════════════════════════════════════════════════════════════════════
# 2. API CLIENT ADDITIONS
# ═══════════════════════════════════════════════════════════════════════════

API_CLIENT_SOCIAL_METHODS = r"""
  // ─── Creature Friendships ─────────────────────────────────

  Future<Map<String, dynamic>> sendFriendshipRequest({
    required String fromCreatureId,
    required String toCreatureId,
    String? metInRabble,
  }) async {
    final body = <String, dynamic>{
      'from_creature_id': fromCreatureId,
      'to_creature_id': toCreatureId,
    };
    if (metInRabble != null) body['met_in_rabble'] = metInRabble;

    final response = await _client.post(
      Uri.parse('$baseUrl/api/creature-friendships'),
      headers: _headers,
      body: jsonEncode(body),
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> acceptFriendship(String friendshipId) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/creature-friendships/$friendshipId/accept'),
      headers: _headers,
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> declineFriendship(String friendshipId) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/creature-friendships/$friendshipId/decline'),
      headers: _headers,
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<void> removeFriendship(String friendshipId) async {
    final response = await _client.delete(
      Uri.parse('$baseUrl/api/creature-friendships/$friendshipId'),
      headers: _headers,
    );
    _checkResponse(response);
  }

  Future<List<Map<String, dynamic>>> listCreatureFriends(
    String creatureId, {
    int limit = 50,
    int offset = 0,
  }) async {
    final uri = Uri.parse('$baseUrl/api/creatures/$creatureId/friends')
        .replace(queryParameters: {
      'limit': limit.toString(),
      'offset': offset.toString(),
    });
    final response = await _client.get(uri, headers: _headers);
    _checkResponse(response);
    final data = jsonDecode(response.body) as Map<String, dynamic>;
    return (data['friends'] as List<dynamic>)
        .cast<Map<String, dynamic>>();
  }

  Future<List<Map<String, dynamic>>> pendingFriendshipRequests() async {
    final response = await _client.get(
      Uri.parse('$baseUrl/api/creature-friendships/pending'),
      headers: _headers,
    );
    _checkResponse(response);
    final data = jsonDecode(response.body) as Map<String, dynamic>;
    return (data['pending_requests'] as List<dynamic>)
        .cast<Map<String, dynamic>>();
  }

  // ─── Creature Invites ("come fly with me") ────────────────

  Future<Map<String, dynamic>> sendCreatureInvite({
    required String fromCreatureId,
    required String toCreatureId,
    String? message,
  }) async {
    final body = <String, dynamic>{
      'from_creature_id': fromCreatureId,
      'to_creature_id': toCreatureId,
    };
    if (message != null) body['message'] = message;

    final response = await _client.post(
      Uri.parse('$baseUrl/api/creature-invites'),
      headers: _headers,
      body: jsonEncode(body),
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> acceptCreatureInvite(String inviteId) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/creature-invites/$inviteId/accept'),
      headers: _headers,
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<Map<String, dynamic>> declineCreatureInvite(String inviteId) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/creature-invites/$inviteId/decline'),
      headers: _headers,
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<List<Map<String, dynamic>>> pendingCreatureInvites() async {
    final response = await _client.get(
      Uri.parse('$baseUrl/api/creature-invites/pending'),
      headers: _headers,
    );
    _checkResponse(response);
    final data = jsonDecode(response.body) as Map<String, dynamic>;
    return (data['invites'] as List<dynamic>)
        .cast<Map<String, dynamic>>();
  }

  // ─── Rabble Recap ─────────────────────────────────────────

  Future<Map<String, dynamic>> getRabbleRecap(
    String rabbleId,
    String creatureId,
  ) async {
    final response = await _client.get(
      Uri.parse('$baseUrl/api/rabble/$rabbleId/recap/$creatureId'),
      headers: _headers,
    );
    _checkResponse(response);
    return jsonDecode(response.body) as Map<String, dynamic>;
  }

  Future<void> recordCoPresence(String rabbleId, String creatureId) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/rabble/$rabbleId/co-presence'),
      headers: _headers,
      body: jsonEncode({'creature_id': creatureId}),
    );
    _checkResponse(response);
  }

  // ─── Social Visibility ────────────────────────────────────

  Future<void> updateSocialVisibility(String visibility) async {
    final response = await _client.put(
      Uri.parse('$baseUrl/api/users/social-visibility'),
      headers: _headers,
      body: jsonEncode({'social_visibility': visibility}),
    );
    _checkResponse(response);
  }

  // ─── Activity Feed ────────────────────────────────────────

  Future<List<Map<String, dynamic>>> getActivityFeed({
    String? before,
    int limit = 50,
  }) async {
    final params = <String, String>{'limit': limit.toString()};
    if (before != null) params['before'] = before;

    final uri = Uri.parse('$baseUrl/api/feed/events')
        .replace(queryParameters: params);
    final response = await _client.get(uri, headers: _headers);
    _checkResponse(response);
    final data = jsonDecode(response.body) as Map<String, dynamic>;
    return (data['events'] as List<dynamic>)
        .cast<Map<String, dynamic>>();
  }
"""

# ═══════════════════════════════════════════════════════════════════════════
# 3. RABBLE RECAP SCREEN
# ═══════════════════════════════════════════════════════════════════════════

RABBLE_RECAP_DART = r"""import 'package:flutter/material.dart';
import 'package:cached_network_image/cached_network_image.dart';
import '../models/creature_friendship.dart';
import '../services/api_client.dart';
import '../theme/rabble_theme.dart';

/// Post-rabble recap screen: "You met these creatures"
/// Shows all creatures that were co-present, with befriend actions.
class RabbleRecapScreen extends StatefulWidget {
  final ApiClient api;
  final String rabbleId;
  final String creatureId;
  final String? rabbleName;

  const RabbleRecapScreen({
    super.key,
    required this.api,
    required this.rabbleId,
    required this.creatureId,
    this.rabbleName,
  });

  @override
  State<RabbleRecapScreen> createState() => _RabbleRecapScreenState();
}

class _RabbleRecapScreenState extends State<RabbleRecapScreen> {
  bool _loading = true;
  String? _error;
  List<RecapCreature> _creatures = [];
  String _rabbleName = '';

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() { _loading = true; _error = null; });
    try {
      final data = await widget.api.getRabbleRecap(
        widget.rabbleId,
        widget.creatureId,
      );
      _rabbleName = data['rabble_name'] as String? ?? widget.rabbleName ?? 'Rabble';
      final list = data['creatures_met'] as List<dynamic>;
      _creatures = list
          .map((j) => RecapCreature.fromJson(j as Map<String, dynamic>))
          .toList();
    } catch (e) {
      _error = e.toString();
    }
    if (mounted) setState(() => _loading = false);
  }

  Future<void> _befriend(RecapCreature creature) async {
    try {
      await widget.api.sendFriendshipRequest(
        fromCreatureId: widget.creatureId,
        toCreatureId: creature.creatureId,
        metInRabble: widget.rabbleId,
      );
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Friend request sent to ${creature.displayName}!'),
            backgroundColor: RabbleTheme.mint,
          ),
        );
        _load(); // Refresh to update friendship status
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Could not send request: $e'),
            backgroundColor: RabbleTheme.coral,
          ),
        );
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: RabbleTheme.bg0,
      appBar: AppBar(
        title: Text('You met in $_rabbleName'),
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _error != null
              ? Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(Icons.error_outline, color: RabbleTheme.coral, size: 48),
                      const SizedBox(height: 12),
                      Text(_error!, style: TextStyle(color: RabbleTheme.fg2)),
                      const SizedBox(height: 12),
                      TextButton(onPressed: _load, child: const Text('Retry')),
                    ],
                  ),
                )
              : _creatures.isEmpty
                  ? Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(Icons.group_outlined, color: RabbleTheme.fg3, size: 64),
                          const SizedBox(height: 16),
                          Text(
                            'No other creatures were here',
                            style: TextStyle(color: RabbleTheme.fg2, fontSize: 16),
                          ),
                        ],
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.all(16),
                      itemCount: _creatures.length + 1, // +1 for header
                      itemBuilder: (context, index) {
                        if (index == 0) return _buildHeader();
                        return _buildCreatureCard(_creatures[index - 1]);
                      },
                    ),
    );
  }

  Widget _buildHeader() {
    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${_creatures.length} creature${_creatures.length == 1 ? '' : 's'} met',
            style: TextStyle(
              color: RabbleTheme.fg0,
              fontSize: 22,
              fontWeight: FontWeight.w300,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            'Befriend creatures you enjoyed flying with',
            style: TextStyle(color: RabbleTheme.fg3, fontSize: 14),
          ),
        ],
      ),
    );
  }

  Widget _buildCreatureCard(RecapCreature creature) {
    final isFriend = creature.alreadyFriends;
    final isPending = creature.friendshipStatus == 'pending';

    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            // Creature avatar
            Container(
              width: 56,
              height: 56,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(12),
                color: RabbleTheme.bg2,
                border: Border.all(
                  color: isFriend
                      ? RabbleTheme.mint.withValues(alpha: 0.6)
                      : RabbleTheme.bg3,
                ),
              ),
              child: creature.assetPath != null
                  ? ClipRRect(
                      borderRadius: BorderRadius.circular(11),
                      child: CachedNetworkImage(
                        imageUrl: creature.assetPath!,
                        fit: BoxFit.cover,
                      ),
                    )
                  : Center(
                      child: RabbleTheme.speciesIconWidget(
                        creature.speciesGroup ?? 'butterfly',
                        size: 28,
                      ),
                    ),
            ),
            const SizedBox(width: 16),
            // Info
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    creature.displayName,
                    style: TextStyle(
                      color: RabbleTheme.fg0,
                      fontSize: 16,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    creature.ownerLabel,
                    style: TextStyle(color: RabbleTheme.fg3, fontSize: 13),
                  ),
                  if (creature.overlapDuration != null) ...[
                    const SizedBox(height: 4),
                    Text(
                      _formatDuration(creature.overlapDuration!),
                      style: TextStyle(color: RabbleTheme.fg3, fontSize: 12),
                    ),
                  ],
                ],
              ),
            ),
            // Action
            if (isFriend)
              Chip(
                label: const Text('Friends'),
                backgroundColor: RabbleTheme.mint.withValues(alpha: 0.15),
                side: BorderSide(color: RabbleTheme.mint.withValues(alpha: 0.3)),
                labelStyle: TextStyle(color: RabbleTheme.mint, fontSize: 12),
              )
            else if (isPending)
              Chip(
                label: const Text('Pending'),
                backgroundColor: RabbleTheme.amber.withValues(alpha: 0.15),
                side: BorderSide(color: RabbleTheme.amber.withValues(alpha: 0.3)),
                labelStyle: TextStyle(color: RabbleTheme.amber, fontSize: 12),
              )
            else
              FilledButton.icon(
                onPressed: () => _befriend(creature),
                icon: const Icon(Icons.favorite_border, size: 16),
                label: const Text('Befriend'),
                style: FilledButton.styleFrom(
                  backgroundColor: RabbleTheme.bg2,
                  foregroundColor: RabbleTheme.mint,
                  side: BorderSide(color: RabbleTheme.mint.withValues(alpha: 0.4)),
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  textStyle: const TextStyle(fontSize: 13),
                ),
              ),
          ],
        ),
      ),
    );
  }

  String _formatDuration(Duration d) {
    if (d.inHours > 0) return 'Together for ${d.inHours}h ${d.inMinutes.remainder(60)}m';
    if (d.inMinutes > 0) return 'Together for ${d.inMinutes}m';
    return 'Briefly met';
  }
}
"""

# ═══════════════════════════════════════════════════════════════════════════
# 4. CREATURE INVITE BOTTOM SHEET
# ═══════════════════════════════════════════════════════════════════════════

CREATURE_INVITE_SHEET_DART = r"""import 'package:flutter/material.dart';
import 'package:cached_network_image/cached_network_image.dart';
import '../models/creature.dart';
import '../services/api_client.dart';
import '../theme/rabble_theme.dart';

/// "Come fly with me" — invite another creature to your rabble.
/// Shows as a bottom sheet with creature picker and optional message.
class CreatureInviteSheet extends StatefulWidget {
  final ApiClient api;
  final String fromCreatureId;
  final String fromCreatureName;
  final String rabbleId;
  final String rabbleName;

  const CreatureInviteSheet({
    super.key,
    required this.api,
    required this.fromCreatureId,
    required this.fromCreatureName,
    required this.rabbleId,
    required this.rabbleName,
  });

  /// Show the invite sheet as a modal bottom sheet.
  static Future<void> show(
    BuildContext context, {
    required ApiClient api,
    required String fromCreatureId,
    required String fromCreatureName,
    required String rabbleId,
    required String rabbleName,
  }) {
    return showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: RabbleTheme.bg1,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (_) => DraggableScrollableSheet(
        initialChildSize: 0.7,
        minChildSize: 0.4,
        maxChildSize: 0.9,
        expand: false,
        builder: (_, scrollController) => CreatureInviteSheet(
          api: api,
          fromCreatureId: fromCreatureId,
          fromCreatureName: fromCreatureName,
          rabbleId: rabbleId,
          rabbleName: rabbleName,
        ),
      ),
    );
  }

  @override
  State<CreatureInviteSheet> createState() => _CreatureInviteSheetState();
}

class _CreatureInviteSheetState extends State<CreatureInviteSheet> {
  bool _loading = true;
  List<Creature> _creatures = [];
  final _messageController = TextEditingController();
  String? _selectedCreatureId;
  bool _sending = false;

  @override
  void initState() {
    super.initState();
    _loadFriendCreatures();
  }

  @override
  void dispose() {
    _messageController.dispose();
    super.dispose();
  }

  Future<void> _loadFriendCreatures() async {
    setState(() => _loading = true);
    try {
      // Load creatures from contacts and friends — show all non-owned creatures
      final creatures = await widget.api.listCreatures(limit: 200);
      // Filter out own creatures (those already in this rabble can be filtered later)
      _creatures = creatures;
    } catch (_) {}
    if (mounted) setState(() => _loading = false);
  }

  Future<void> _sendInvite() async {
    if (_selectedCreatureId == null) return;
    setState(() => _sending = true);
    try {
      await widget.api.sendCreatureInvite(
        fromCreatureId: widget.fromCreatureId,
        toCreatureId: _selectedCreatureId!,
        message: _messageController.text.isNotEmpty ? _messageController.text : null,
      );
      if (mounted) {
        Navigator.of(context).pop();
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Invite sent! Come fly with me \u2728'),
            backgroundColor: RabbleTheme.mint,
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Could not send invite: $e'),
            backgroundColor: RabbleTheme.coral,
          ),
        );
      }
    }
    if (mounted) setState(() => _sending = false);
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.only(
        bottom: MediaQuery.of(context).viewInsets.bottom,
        left: 20, right: 20, top: 16,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          // Handle
          Center(
            child: Container(
              width: 40, height: 4,
              decoration: BoxDecoration(
                color: RabbleTheme.fg3,
                borderRadius: BorderRadius.circular(2),
              ),
            ),
          ),
          const SizedBox(height: 20),
          // Title
          Text(
            'Come fly with me!',
            style: TextStyle(
              color: RabbleTheme.fg0, fontSize: 20, fontWeight: FontWeight.w500,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            '${widget.fromCreatureName} invites a creature to ${widget.rabbleName}',
            style: TextStyle(color: RabbleTheme.fg3, fontSize: 14),
          ),
          const SizedBox(height: 20),
          // Message field
          TextField(
            controller: _messageController,
            decoration: InputDecoration(
              hintText: 'Add a message (optional)',
              hintStyle: TextStyle(color: RabbleTheme.fg3),
            ),
            maxLength: 200,
            style: TextStyle(color: RabbleTheme.fg1),
          ),
          const SizedBox(height: 16),
          // Creature list
          if (_loading)
            const Center(child: CircularProgressIndicator())
          else if (_creatures.isEmpty)
            Center(
              child: Text(
                'No creatures to invite',
                style: TextStyle(color: RabbleTheme.fg3),
              ),
            )
          else
            Expanded(
              child: ListView.builder(
                itemCount: _creatures.length,
                itemBuilder: (context, index) {
                  final c = _creatures[index];
                  final isSelected = _selectedCreatureId == c.creatureId;
                  return ListTile(
                    leading: Container(
                      width: 40, height: 40,
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(8),
                        color: RabbleTheme.bg2,
                        border: Border.all(
                          color: isSelected ? RabbleTheme.mint : RabbleTheme.bg3,
                          width: isSelected ? 2 : 1,
                        ),
                      ),
                      child: Center(
                        child: RabbleTheme.speciesIconWidget(
                          c.speciesGroup, size: 20,
                        ),
                      ),
                    ),
                    title: Text(
                      c.specimenName ?? 'Unknown',
                      style: TextStyle(
                        color: isSelected ? RabbleTheme.mint : RabbleTheme.fg0,
                        fontWeight: isSelected ? FontWeight.w600 : FontWeight.w400,
                      ),
                    ),
                    subtitle: Text(
                      c.scientificName ?? '',
                      style: TextStyle(color: RabbleTheme.fg3, fontSize: 12),
                    ),
                    selected: isSelected,
                    onTap: () => setState(() => _selectedCreatureId = c.creatureId),
                  );
                },
              ),
            ),
          const SizedBox(height: 12),
          // Send button
          SizedBox(
            width: double.infinity,
            child: FilledButton(
              onPressed: _selectedCreatureId != null && !_sending ? _sendInvite : null,
              child: _sending
                  ? const SizedBox(
                      width: 20, height: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('Send Invite'),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
"""

# ═══════════════════════════════════════════════════════════════════════════
# 5. ACTIVITY FEED WIDGET
# ═══════════════════════════════════════════════════════════════════════════

ACTIVITY_FEED_DART = r"""import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import '../models/creature_friendship.dart';
import '../services/api_client.dart';
import '../theme/rabble_theme.dart';

/// Activity feed widget with SSE streaming support.
/// Replaces the old 30s polling feed with real-time push.
class ActivityFeedWidget extends StatefulWidget {
  final ApiClient api;
  final String baseUrl;
  final String? token;

  const ActivityFeedWidget({
    super.key,
    required this.api,
    required this.baseUrl,
    this.token,
  });

  @override
  State<ActivityFeedWidget> createState() => _ActivityFeedWidgetState();
}

class _ActivityFeedWidgetState extends State<ActivityFeedWidget> {
  final List<ActivityEvent> _events = [];
  bool _loading = true;
  String? _error;
  StreamSubscription<String>? _sseSub;
  final ScrollController _scrollController = ScrollController();

  @override
  void initState() {
    super.initState();
    _loadInitial();
    _connectSSE();
  }

  @override
  void dispose() {
    _sseSub?.cancel();
    _scrollController.dispose();
    super.dispose();
  }

  Future<void> _loadInitial() async {
    setState(() { _loading = true; _error = null; });
    try {
      final raw = await widget.api.getActivityFeed(limit: 50);
      _events.clear();
      _events.addAll(raw.map((j) => ActivityEvent.fromJson(j)));
    } catch (e) {
      _error = e.toString();
    }
    if (mounted) setState(() => _loading = false);
  }

  void _connectSSE() async {
    try {
      final uri = Uri.parse('${widget.baseUrl}/api/feed/stream');
      final request = http.Request('GET', uri);
      if (widget.token != null) {
        request.headers['Authorization'] = 'Bearer ${widget.token}';
      }
      final client = http.Client();
      final response = await client.send(request);

      _sseSub = response.stream
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .where((line) => line.startsWith('data:'))
          .map((line) => line.substring(5).trim())
          .where((data) => data.isNotEmpty)
          .listen((data) {
        try {
          final json = jsonDecode(data) as Map<String, dynamic>;
          final event = ActivityEvent.fromJson(json);
          if (mounted) {
            setState(() {
              // Prepend new events at the top
              _events.insert(0, event);
              // Cap at 200 events in memory
              if (_events.length > 200) _events.removeLast();
            });
          }
        } catch (_) {}
      }, onError: (_) {
        // Reconnect after 5 seconds on error
        Future.delayed(const Duration(seconds: 5), () {
          if (mounted) _connectSSE();
        });
      });
    } catch (_) {
      // Fallback: retry connection
      Future.delayed(const Duration(seconds: 5), () {
        if (mounted) _connectSSE();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_loading && _events.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null && _events.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.wifi_off, color: RabbleTheme.fg3, size: 48),
            const SizedBox(height: 12),
            Text(_error!, style: TextStyle(color: RabbleTheme.fg2)),
            TextButton(onPressed: _loadInitial, child: const Text('Retry')),
          ],
        ),
      );
    }
    if (_events.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.explore_outlined, color: RabbleTheme.fg3, size: 64),
            const SizedBox(height: 16),
            Text(
              'No activity yet',
              style: TextStyle(color: RabbleTheme.fg2, fontSize: 16),
            ),
            const SizedBox(height: 4),
            Text(
              'Fly your creatures and join rabbles to see activity here',
              style: TextStyle(color: RabbleTheme.fg3, fontSize: 13),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      );
    }

    return RefreshIndicator(
      onRefresh: _loadInitial,
      child: ListView.builder(
        controller: _scrollController,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        itemCount: _events.length,
        itemBuilder: (context, index) => _buildEventTile(_events[index]),
      ),
    );
  }

  Widget _buildEventTile(ActivityEvent event) {
    final icon = _eventIcon(event.eventType);
    final color = _eventColor(event);

    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Container(
        decoration: BoxDecoration(
          color: event.relationshipPriority > 0
              ? color.withValues(alpha: 0.06)
              : RabbleTheme.bg1,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: event.relationshipPriority > 0
                ? color.withValues(alpha: 0.2)
                : RabbleTheme.bg3.withValues(alpha: 0.5),
          ),
        ),
        padding: const EdgeInsets.all(14),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Event icon
            Container(
              width: 36, height: 36,
              decoration: BoxDecoration(
                color: color.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Icon(icon, color: color, size: 18),
            ),
            const SizedBox(width: 12),
            // Content
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Title + relationship badge
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          event.title,
                          style: TextStyle(
                            color: RabbleTheme.fg0,
                            fontSize: 14,
                            fontWeight: FontWeight.w500,
                          ),
                        ),
                      ),
                      if (event.relationshipLabel.isNotEmpty)
                        Container(
                          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                          decoration: BoxDecoration(
                            color: color.withValues(alpha: 0.15),
                            borderRadius: BorderRadius.circular(4),
                          ),
                          child: Text(
                            event.relationshipLabel,
                            style: TextStyle(color: color, fontSize: 10, fontWeight: FontWeight.w600),
                          ),
                        ),
                    ],
                  ),
                  if (event.body != null) ...[
                    const SizedBox(height: 4),
                    Text(
                      event.body!,
                      style: TextStyle(color: RabbleTheme.fg2, fontSize: 13),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                  const SizedBox(height: 6),
                  // Timestamp + rabble context
                  Row(
                    children: [
                      Text(
                        _timeAgo(event.createdAt),
                        style: TextStyle(color: RabbleTheme.fg3, fontSize: 11),
                      ),
                      if (event.rabbleName != null) ...[
                        Text(' \u00b7 ', style: TextStyle(color: RabbleTheme.fg3, fontSize: 11)),
                        Flexible(
                          child: Text(
                            event.rabbleName!,
                            style: TextStyle(color: RabbleTheme.fg3, fontSize: 11),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ],
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  IconData _eventIcon(String eventType) {
    switch (eventType) {
      case 'creature_minted': return Icons.auto_awesome;
      case 'creature_flew': return Icons.flight_takeoff;
      case 'creature_landed': return Icons.flight_land;
      case 'creature_perched': return Icons.place;
      case 'rabble_created': return Icons.groups;
      case 'rabble_joined': return Icons.group_add;
      case 'rabble_left': return Icons.exit_to_app;
      case 'rabble_completed': return Icons.check_circle_outline;
      case 'friendship_requested': return Icons.favorite_border;
      case 'friendship_accepted': return Icons.favorite;
      case 'creature_invited': return Icons.mail_outline;
      case 'creature_invite_accepted': return Icons.mark_email_read;
      case 'flight_planned': return Icons.map_outlined;
      case 'observation_recorded': return Icons.visibility;
      case 'creature_gifted': return Icons.card_giftcard;
      case 'chat_message': return Icons.chat_bubble_outline;
      default: return Icons.circle;
    }
  }

  Color _eventColor(ActivityEvent event) {
    if (event.isOwnCreature) return RabbleTheme.amber;
    if (event.isFriendCreature) return RabbleTheme.mint;
    if (event.isContact) return RabbleTheme.sky;

    switch (event.eventType) {
      case 'friendship_requested':
      case 'friendship_accepted':
        return RabbleTheme.coral;
      case 'creature_invited':
      case 'creature_invite_accepted':
        return RabbleTheme.violet;
      case 'rabble_created':
      case 'rabble_joined':
        return RabbleTheme.mint;
      default:
        return RabbleTheme.fg2;
    }
  }

  String _timeAgo(DateTime dt) {
    final diff = DateTime.now().difference(dt);
    if (diff.inSeconds < 60) return 'just now';
    if (diff.inMinutes < 60) return '${diff.inMinutes}m ago';
    if (diff.inHours < 24) return '${diff.inHours}h ago';
    if (diff.inDays < 7) return '${diff.inDays}d ago';
    return '${dt.month}/${dt.day}';
  }
}
"""

# ═══════════════════════════════════════════════════════════════════════════
# 6. FRIENDSHIP REQUEST CARD WIDGET
# ═══════════════════════════════════════════════════════════════════════════

FRIENDSHIP_REQUEST_CARD_DART = r"""import 'package:flutter/material.dart';
import 'package:cached_network_image/cached_network_image.dart';
import '../models/creature_friendship.dart';
import '../services/api_client.dart';
import '../theme/rabble_theme.dart';

/// Card widget for displaying and acting on a pending friendship request.
class FriendshipRequestCard extends StatefulWidget {
  final FriendshipRequest request;
  final ApiClient api;
  final VoidCallback? onResponded;

  const FriendshipRequestCard({
    super.key,
    required this.request,
    required this.api,
    this.onResponded,
  });

  @override
  State<FriendshipRequestCard> createState() => _FriendshipRequestCardState();
}

class _FriendshipRequestCardState extends State<FriendshipRequestCard> {
  bool _responding = false;

  Future<void> _accept() async {
    setState(() => _responding = true);
    try {
      await widget.api.acceptFriendship(widget.request.friendshipId);
      widget.onResponded?.call();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('You and ${widget.request.fromCreatureName ?? "them"} are now friends!'),
            backgroundColor: RabbleTheme.mint,
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Error: $e'), backgroundColor: RabbleTheme.coral),
        );
      }
    }
    if (mounted) setState(() => _responding = false);
  }

  Future<void> _decline() async {
    setState(() => _responding = true);
    try {
      await widget.api.declineFriendship(widget.request.friendshipId);
      widget.onResponded?.call();
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Error: $e'), backgroundColor: RabbleTheme.coral),
        );
      }
    }
    if (mounted) setState(() => _responding = false);
  }

  @override
  Widget build(BuildContext context) {
    final req = widget.request;
    return Card(
      margin: const EdgeInsets.only(bottom: 10),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Row(
          children: [
            // Avatar
            Container(
              width: 48, height: 48,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(10),
                color: RabbleTheme.bg2,
              ),
              child: req.fromAssetPath != null
                  ? ClipRRect(
                      borderRadius: BorderRadius.circular(9),
                      child: CachedNetworkImage(
                        imageUrl: req.fromAssetPath!,
                        fit: BoxFit.cover,
                      ),
                    )
                  : Center(
                      child: RabbleTheme.speciesIconWidget(
                        req.fromSpeciesGroup ?? 'butterfly', size: 24,
                      ),
                    ),
            ),
            const SizedBox(width: 14),
            // Info
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '${req.fromCreatureName ?? "A creature"} wants to be friends!',
                    style: TextStyle(
                      color: RabbleTheme.fg0, fontSize: 14, fontWeight: FontWeight.w500,
                    ),
                  ),
                  if (req.rabbleName != null) ...[
                    const SizedBox(height: 2),
                    Text(
                      'Met in ${req.rabbleName}',
                      style: TextStyle(color: RabbleTheme.fg3, fontSize: 12),
                    ),
                  ],
                  if (req.fromOwnerName != null) ...[
                    const SizedBox(height: 2),
                    Text(
                      'Owner: ${req.fromOwnerName}',
                      style: TextStyle(color: RabbleTheme.fg3, fontSize: 12),
                    ),
                  ],
                ],
              ),
            ),
            // Actions
            if (_responding)
              const SizedBox(
                width: 24, height: 24,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            else
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  IconButton(
                    onPressed: _decline,
                    icon: Icon(Icons.close, color: RabbleTheme.fg3, size: 20),
                    tooltip: 'Decline',
                    style: IconButton.styleFrom(
                      backgroundColor: RabbleTheme.bg2,
                      fixedSize: const Size(36, 36),
                    ),
                  ),
                  const SizedBox(width: 8),
                  IconButton(
                    onPressed: _accept,
                    icon: Icon(Icons.favorite, color: RabbleTheme.mint, size: 20),
                    tooltip: 'Accept',
                    style: IconButton.styleFrom(
                      backgroundColor: RabbleTheme.mint.withValues(alpha: 0.15),
                      fixedSize: const Size(36, 36),
                    ),
                  ),
                ],
              ),
          ],
        ),
      ),
    );
  }
}
"""

# ═══════════════════════════════════════════════════════════════════════════
# 7. MAIN — WRITE ALL FILES
# ═══════════════════════════════════════════════════════════════════════════


def write_file(path, content):
    """Write content to file, creating parent dirs if needed."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content.lstrip("\n"))
    print(f"  ✓ {path}")


def patch_api_client():
    """Append social layer methods to api_client.dart before the closing helpers."""
    api_path = os.path.join(RABBLE_ROOT, "lib/services/api_client.dart")
    with open(api_path, "r") as f:
        content = f.read()

    # Check if already patched
    if "sendFriendshipRequest" in content:
        print("  ⊘ api_client.dart already patched, skipping")
        return

    # Insert before the Helpers section
    marker = "  // ─── Helpers ───"
    if marker not in content:
        # Fallback: insert before _checkResponse
        marker = "  void _checkResponse"

    if marker in content:
        idx = content.index(marker)
        content = content[:idx] + API_CLIENT_SOCIAL_METHODS + "\n" + content[idx:]
        with open(api_path, "w") as f:
            f.write(content)
        print(f"  ✓ {api_path} (patched with social layer methods)")
    else:
        print(f"  ✗ Could not find insertion point in {api_path}")


def main():
    print("Generating Flutter social layer files...\n")

    # Models
    write_file(
        os.path.join(RABBLE_ROOT, "lib/models/creature_friendship.dart"),
        CREATURE_FRIENDSHIP_DART,
    )

    # Screens
    write_file(
        os.path.join(RABBLE_ROOT, "lib/screens/rabble_recap.dart"),
        RABBLE_RECAP_DART,
    )

    # Widgets
    write_file(
        os.path.join(RABBLE_ROOT, "lib/widgets/creature_invite_sheet.dart"),
        CREATURE_INVITE_SHEET_DART,
    )
    write_file(
        os.path.join(RABBLE_ROOT, "lib/widgets/friendship_request_card.dart"),
        FRIENDSHIP_REQUEST_CARD_DART,
    )
    write_file(
        os.path.join(RABBLE_ROOT, "lib/widgets/activity_feed.dart"),
        ACTIVITY_FEED_DART,
    )

    # Patch API client
    patch_api_client()

    print("\n✅ All social layer Flutter files generated.")
    print("\nNext steps:")
    print("  1. cd /home/ilabra/rabble")
    print("  2. flutter pub get")
    print(
        "  3. Wire widgets into screens (rabble_chat.dart, creature_actions.dart, explore_screen.dart)"
    )
    print("  4. flutter build web --release")
    print("  5. Copy to fermi/rabble-web/ and deploy")


if __name__ == "__main__":
    main()
