#!/usr/bin/env python3
"""Wire emit_activity_event calls into remaining handlers.

Adds fire-and-forget activity event emission to:
  - fly_handler → creature_flew
  - end_flight_handler → creature_landed
  - plan_flight_handler → flight_planned
  - perch_handler → creature_perched
  - transfer_creature_handler → creature_gifted
  - post_rabble_message → chat_message (lightweight, only system/narrator)

Run from fermi root:
    python3 scripts/wire_emits.py
"""

import os
import re
import sys

STATE_RS = "/home/ilabra/fermi/src/handlers/creatures/state.rs"
IDENTITY_RS = "/home/ilabra/fermi/src/handlers/creatures/identity.rs"
CHAT_RS = "/home/ilabra/fermi/src/handlers/rabble_chat.rs"


def read_file(path):
    with open(path, "r") as f:
        return f.read()


def write_file(path, content):
    with open(path, "w") as f:
        f.write(content)


def find_ok_json_before(content, marker_line_text, search_start=0):
    """Find the Ok(Json(...)) block that ends a handler, searching backwards
    from a known marker text. Returns the index of 'Ok(Json' or -1."""
    idx = content.find(marker_line_text, search_start)
    if idx == -1:
        return -1
    # Search backwards from idx for Ok(Json
    search_region = content[max(0, idx - 2000) : idx]
    last_ok = search_region.rfind("Ok(Json")
    if last_ok == -1:
        return -1
    return max(0, idx - 2000) + last_ok


def insert_before_ok_json(content, ok_json_idx, emit_block):
    """Insert an emit block right before Ok(Json(...)). We insert before
    the line containing Ok(Json."""
    # Find the start of the line containing Ok(Json
    line_start = content.rfind("\n", 0, ok_json_idx)
    if line_start == -1:
        line_start = 0
    else:
        line_start += 1  # skip the newline itself

    return content[:line_start] + emit_block + "\n" + content[line_start:]


def make_emit_block(
    pool_src,
    user_id_var,
    creature_id_expr,
    event_type,
    rabble_id_expr,
    target_creature_expr,
    title_format,
    indent="    ",
):
    """Generate a fire-and-forget emit block."""
    lines = []
    lines.append(f"{indent}// Emit activity event (fire-and-forget)")
    lines.append(f"{indent}{{")
    lines.append(f"{indent}    let _pool_ae = {pool_src}.clone();")
    lines.append(f"{indent}    let _uid_ae = {user_id_var}.clone();")

    # Handle creature_id
    if creature_id_expr.startswith("Some("):
        inner = creature_id_expr[5:-1]
        lines.append(f"{indent}    let _cid_ae = {inner};")
        cid_arg = "Some(_cid_ae)"
    else:
        cid_arg = creature_id_expr

    # Handle title - might need cloned vars
    title_line = title_format
    # Extract variables that need cloning for the spawn
    clone_vars = re.findall(r"\{(\w+)\}", title_format)

    for var in clone_vars:
        lines.append(f"{indent}    let _{var}_ae = {var}.clone();")

    # Build the title string for inside the spawn
    spawn_title = title_format
    for var in clone_vars:
        spawn_title = spawn_title.replace("{" + var + "}", "{_" + var + "_ae}")

    lines.append(f"{indent}    tokio::spawn(async move {{")
    lines.append(f"{indent}        crate::handlers::social::emit_activity_event(")
    lines.append(f"{indent}            &_pool_ae,")
    lines.append(f"{indent}            &_uid_ae,")

    if cid_arg == "Some(_cid_ae)":
        lines.append(f"{indent}            Some(_cid_ae),")
    else:
        lines.append(f"{indent}            {cid_arg},")

    lines.append(f'{indent}            "{event_type}",')
    lines.append(f"{indent}            {rabble_id_expr},")
    lines.append(f"{indent}            {target_creature_expr},")
    lines.append(f'{indent}            &format!("{spawn_title}"),')
    lines.append(f"{indent}            None,")
    lines.append(f"{indent}            None,")
    lines.append(f"{indent}        )")
    lines.append(f"{indent}        .await;")
    lines.append(f"{indent}    }});")
    lines.append(f"{indent}}}")
    lines.append("")

    return "\n".join(lines) + "\n"


def patch_state_rs():
    """Add emit calls to fly, end_flight, plan_flight, perch handlers."""
    content = read_file(STATE_RS)
    original = content
    patched_count = 0

    # ─── 1. record_flight_handler → creature_flew ───
    # Find the Ok(Json that returns flight_id, creature_id, h3_cell...
    marker = '"started_at": now.to_rfc3339(),'
    if (
        "creature_flew" not in content[: content.find(marker) + 200]
        if marker in content
        else True
    ):
        ok_idx = find_ok_json_before(content, marker)
        if ok_idx > 0:
            emit = make_emit_block(
                pool_src="state.memory_store.pool()",
                user_id_var="user_id",
                creature_id_expr="Some(req.creature_id)",
                event_type="creature_flew",
                rabble_id_expr="None",
                target_creature_expr="None",
                title_format="Creature flew to {h3_cell}",
            )
            # We need h3_cell available - check if it's in scope
            # h3_cell is defined earlier as: let h3_cell = ...
            # But it might be a &str, we need to handle that
            # Actually let's use a simpler title
            emit = make_emit_block(
                pool_src="state.memory_store.pool()",
                user_id_var="user_id",
                creature_id_expr="Some(req.creature_id)",
                event_type="creature_flew",
                rabble_id_expr="None",
                target_creature_expr="None",
                title_format="Creature took flight",
            )
            content = insert_before_ok_json(content, ok_idx, emit)
            patched_count += 1
            print("  ✓ record_flight_handler → creature_flew")
        else:
            print("  ✗ record_flight_handler: could not find Ok(Json)")
    else:
        print("  ⊘ record_flight_handler already emits")

    # ─── 2. end_flight_handler → creature_landed ───
    marker2 = '"has_path": req.path_samples.is_some()'
    if "creature_landed" not in content:
        ok_idx = find_ok_json_before(content, marker2)
        if ok_idx > 0:
            # In end_flight_handler, we have: flight_id, creature_id (from the DB row)
            # The handler reads creature_id from the flight row
            # Let's find what variable holds creature_id
            # Looking at the handler: let creature_id: Uuid = row.get("creature_id");
            # And user_id comes from: let user_id = principal.user_id();
            # Actually end_flight_handler doesn't extract principal.user_id at top
            # It uses: principal: AuthPrincipal but may not bind user_id
            # Let me check - it gets pool and now, then queries.
            # We need a safe approach: check if user_id exists
            # The handler signature has principal: AuthPrincipal
            # Let's look for how it gets user_id
            handler_start = content.find("pub async fn end_flight_handler")
            handler_chunk = content[handler_start : handler_start + 800]

            if "let user_id" not in handler_chunk:
                # Need to figure out the user id source
                # end_flight might use principal differently
                # Let's use a minimal emit that just uses flight_id
                emit = []
                emit.append("    // Emit activity event (fire-and-forget)")
                emit.append("    {")
                emit.append("        let _pool_ae = state.memory_store.pool().clone();")
                emit.append("        let _uid_ae = principal.user_id();")
                emit.append("        let _fid_ae = flight_id;")
                # Get creature_id - it's queried from the flight row
                # We need to find where creature_id is extracted
                if "let creature_id" in handler_chunk:
                    emit.append("        let _cid_ae = creature_id;")
                    cid_line = "            Some(_cid_ae),"
                else:
                    cid_line = "            None,"
                emit.append("        tokio::spawn(async move {")
                emit.append("            crate::handlers::social::emit_activity_event(")
                emit.append("                &_pool_ae,")
                emit.append("                &_uid_ae,")
                emit.append(cid_line)
                emit.append('                "creature_landed",')
                emit.append("                None,")
                emit.append("                None,")
                emit.append('                "Creature landed",')
                emit.append("                None,")
                emit.append("                None,")
                emit.append("            )")
                emit.append("            .await;")
                emit.append("        });")
                emit.append("    }")
                emit.append("")
                emit_text = "\n".join(emit) + "\n"
            else:
                emit_text = make_emit_block(
                    pool_src="state.memory_store.pool()",
                    user_id_var="user_id",
                    creature_id_expr="None",
                    event_type="creature_landed",
                    rabble_id_expr="None",
                    target_creature_expr="None",
                    title_format="Creature landed",
                )
            content = insert_before_ok_json(content, ok_idx, emit_text)
            patched_count += 1
            print("  ✓ end_flight_handler → creature_landed")
        else:
            print("  ✗ end_flight_handler: could not find Ok(Json)")
    else:
        print("  ⊘ end_flight_handler already emits")

    # ─── 3. plan_flight_handler → flight_planned ───
    marker3 = '"status": "processing"'
    if "flight_planned" not in content:
        ok_idx = find_ok_json_before(content, marker3)
        if ok_idx > 0:
            emit = make_emit_block(
                pool_src="state.memory_store.pool()",
                user_id_var="user_id",
                creature_id_expr="Some(req.creature_id)",
                event_type="flight_planned",
                rabble_id_expr="req.swarm_id",
                target_creature_expr="None",
                title_format="Flight plan dispatched for {specimen_name}",
            )
            content = insert_before_ok_json(content, ok_idx, emit)
            patched_count += 1
            print("  ✓ plan_flight_handler → flight_planned")
        else:
            print("  ✗ plan_flight_handler: could not find Ok(Json)")
    else:
        print("  ⊘ plan_flight_handler already emits")

    # ─── 4. perch_handler → creature_perched ───
    # perch_handler returns Ok(Json(response)) where response is built earlier
    # Find it by looking for the handler and its Ok(Json(response))
    perch_start = content.find("pub async fn perch_handler")
    if (
        perch_start > 0
        and "creature_perched" not in content[perch_start : perch_start + 3000]
    ):
        # Find Ok(Json(response)) after perch_handler
        perch_region = content[perch_start : perch_start + 5000]
        ok_offset = perch_region.find("Ok(Json(response))")
        if ok_offset > 0:
            abs_idx = perch_start + ok_offset
            emit = make_emit_block(
                pool_src="state.memory_store.pool()",
                user_id_var="user_id",
                creature_id_expr="Some(creature_id)",
                event_type="creature_perched",
                rabble_id_expr="None",
                target_creature_expr="None",
                title_format="Creature perched at a new location",
            )
            content = insert_before_ok_json(content, abs_idx, emit)
            patched_count += 1
            print("  ✓ perch_handler → creature_perched")
        else:
            print("  ✗ perch_handler: could not find Ok(Json(response))")
    else:
        if perch_start > 0:
            print("  ⊘ perch_handler already emits")
        else:
            print("  ✗ perch_handler: handler not found")

    # ─── 5. fly_handler → creature_flew ───
    fly_start = content.find("pub async fn fly_handler")
    if fly_start > 0 and "creature_flew" not in content[fly_start : fly_start + 8000]:
        fly_region = content[fly_start : fly_start + 8000]
        # fly_handler returns: Ok(Json(json!({ "flight_id": flight_id, "swarm_id": ...
        fly_ok_marker = '"gas_charged": fly_cost'
        fly_ok_offset = fly_region.find(fly_ok_marker)
        if fly_ok_offset > 0:
            abs_idx = fly_start + fly_ok_offset
            ok_json_idx = content.rfind("Ok(Json", max(0, abs_idx - 500), abs_idx)
            if ok_json_idx > 0:
                # fly_handler has: creature_id (from path), user_id, specimen_name
                emit = make_emit_block(
                    pool_src="state.memory_store.pool()",
                    user_id_var="user_id",
                    creature_id_expr="Some(creature_id)",
                    event_type="creature_flew",
                    rabble_id_expr="swarm_id",
                    target_creature_expr="None",
                    title_format="{specimen_name} took flight",
                )
                content = insert_before_ok_json(content, ok_json_idx, emit)
                patched_count += 1
                print("  ✓ fly_handler → creature_flew")
            else:
                print("  ✗ fly_handler: could not find Ok(Json) near gas_charged")
        else:
            print("  ✗ fly_handler: could not find gas_charged marker")
    else:
        if fly_start > 0:
            print("  ⊘ fly_handler already emits creature_flew")
        else:
            print("  ✗ fly_handler: handler not found")

    if content != original:
        write_file(STATE_RS, content)
        print(f"\n  Wrote {patched_count} emit(s) to state.rs")
    else:
        print("\n  No changes to state.rs")

    return patched_count


def patch_identity_rs():
    """Add emit to transfer_creature_handler → creature_gifted."""
    content = read_file(IDENTITY_RS)
    original = content
    patched_count = 0

    if "creature_gifted" not in content:
        # transfer_creature_handler returns Ok(Json(json!({...})))
        # Find it: the handler has "transferred": true in its response
        marker = '"transferred": true'
        ok_idx = find_ok_json_before(content, marker)
        if ok_idx > 0:
            # The handler uses: owner_id (from principal), cid (creature UUID),
            # body.recipient_id, creature row has specimen_name
            emit_lines = []
            emit_lines.append("    // Emit activity event (fire-and-forget)")
            emit_lines.append("    {")
            emit_lines.append(
                "        let _pool_ae = state.memory_store.pool().clone();"
            )
            emit_lines.append("        let _uid_ae = owner_id.clone();")
            emit_lines.append("        let _cid_ae = cid;")
            emit_lines.append("        let _recipient_ae = body.recipient_id.clone();")
            emit_lines.append("        tokio::spawn(async move {")
            emit_lines.append(
                "            crate::handlers::social::emit_activity_event("
            )
            emit_lines.append("                &_pool_ae,")
            emit_lines.append("                &_uid_ae,")
            emit_lines.append("                Some(_cid_ae),")
            emit_lines.append('                "creature_gifted",')
            emit_lines.append("                None,")
            emit_lines.append("                None,")
            emit_lines.append('                "Creature gifted to a new owner",')
            emit_lines.append("                None,")
            emit_lines.append("                None,")
            emit_lines.append("            )")
            emit_lines.append("            .await;")
            emit_lines.append("        });")
            emit_lines.append("    }")
            emit_lines.append("")
            emit_text = "\n".join(emit_lines) + "\n"
            content = insert_before_ok_json(content, ok_idx, emit_text)
            patched_count += 1
            print("  ✓ transfer_creature_handler → creature_gifted")
        else:
            print("  ✗ transfer_creature_handler: could not find Ok(Json)")
    else:
        print("  ⊘ transfer_creature_handler already emits")

    if content != original:
        write_file(IDENTITY_RS, content)
        print(f"\n  Wrote {patched_count} emit(s) to identity.rs")
    else:
        print("\n  No changes to identity.rs")

    return patched_count


def patch_rabble_chat():
    """Add lightweight emit for narrator/system chat messages only.
    Regular user chat messages would be too noisy for the feed."""
    content = read_file(CHAT_RS)
    original = content

    if "emit_activity_event" in content:
        print("  ⊘ rabble_chat.rs already has emit calls")
        return 0

    # Find insert_narrator_message and insert_system_message
    # These are internal helpers that post system/narrator messages
    # We want to emit for narrator messages as they're the interesting ones

    # Find insert_narrator_message return
    marker = "pub async fn insert_narrator_message"
    if marker in content:
        # Find the Ok or the end of this function
        fn_start = content.find(marker)
        fn_region = content[fn_start : fn_start + 1500]

        # Look for the return statement
        ok_idx_local = fn_region.find("Ok(())")
        if ok_idx_local > 0:
            abs_idx = fn_start + ok_idx_local

            emit_lines = []
            emit_lines.append(
                "    // Emit activity event for narrator messages (fire-and-forget)"
            )
            emit_lines.append("    {")
            emit_lines.append("        let _pool_ae = pool.clone();")
            emit_lines.append("        let _swarm_ae = swarm_id;")
            emit_lines.append("        let _content_ae = content.to_string();")
            emit_lines.append("        tokio::spawn(async move {")
            emit_lines.append(
                "            crate::handlers::social::emit_activity_event("
            )
            emit_lines.append("                &_pool_ae,")
            emit_lines.append('                "system",')
            emit_lines.append("                None,")
            emit_lines.append('                "chat_message",')
            emit_lines.append("                Some(_swarm_ae),")
            emit_lines.append("                None,")
            emit_lines.append("                &_content_ae,")
            emit_lines.append("                None,")
            emit_lines.append("                None,")
            emit_lines.append("            )")
            emit_lines.append("            .await;")
            emit_lines.append("        });")
            emit_lines.append("    }")
            emit_lines.append("")
            emit_text = "\n".join(emit_lines) + "\n"

            # Insert before Ok(())
            line_start = content.rfind("\n", 0, abs_idx)
            if line_start == -1:
                line_start = 0
            else:
                line_start += 1
            content = content[:line_start] + emit_text + content[line_start:]
            print("  ✓ insert_narrator_message → chat_message event")
        else:
            print("  ✗ insert_narrator_message: could not find Ok(())")
    else:
        print("  ✗ insert_narrator_message not found")

    if content != original:
        write_file(CHAT_RS, content)
        print("\n  Wrote emit(s) to rabble_chat.rs")
        return 1
    else:
        print("\n  No changes to rabble_chat.rs")
        return 0


def main():
    print("Wiring emit_activity_event into remaining handlers...\n")

    print("═══ state.rs (fly, end_flight, plan_flight, perch, record_flight) ═══")
    c1 = patch_state_rs()

    print("\n═══ identity.rs (transfer_creature) ═══")
    c2 = patch_identity_rs()

    print("\n═══ rabble_chat.rs (narrator messages) ═══")
    c3 = patch_rabble_chat()

    total = c1 + c2 + c3
    print(f"\n{'═' * 60}")
    print(f"✅ Done. {total} handler(s) patched with emit calls.")
    print()
    print("Handlers now emitting activity events:")
    print("  • mint_creature_handler → creature_minted (already done)")
    print("  • host_rabble_handler → rabble_created (already done)")
    print("  • join_swarm_handler → rabble_joined (already done)")
    print("  • record_flight_handler → creature_flew")
    print("  • end_flight_handler → creature_landed")
    print("  • plan_flight_handler → flight_planned")
    print("  • perch_handler → creature_perched")
    print("  • fly_handler → creature_flew")
    print("  • transfer_creature_handler → creature_gifted")
    print("  • insert_narrator_message → chat_message")
    print("  • send_friendship_request → friendship_requested (already done)")
    print("  • accept_friendship → friendship_accepted (already done)")
    print("  • send_creature_invite → creature_invited (already done)")
    print("  • accept_creature_invite → creature_invite_accepted (already done)")
    print()
    print("Next: cargo check, then wire Flutter widgets")


if __name__ == "__main__":
    main()
