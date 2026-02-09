use serial_test::serial;

use crate::common::JanusTest;

#[test]
fn test_plan_add_ticket_simple() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Add ticket to plan
    let output = janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("Added"));
    assert!(output.contains(&ticket_id));

    // Verify ticket is in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(output.contains(&ticket_id));
}

#[test]
fn test_plan_add_ticket_phased() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Add ticket to phase
    let output = janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);
    assert!(output.contains("Added"));

    // Verify ticket is in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(output.contains(&ticket_id));
}

#[test]
fn test_plan_add_ticket_requires_phase_for_phased_plan() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Try to add ticket without --phase (should fail)
    let output = janus.run_failure(&["plan", "add-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("--phase"));
}

#[test]
fn test_plan_add_ticket_duplicate() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Add ticket to plan
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Try to add same ticket again (should fail)
    let output = janus.run_failure(&["plan", "add-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("already"));
}

#[test]
fn test_plan_add_ticket_with_position() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create three tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    let ticket3 = janus
        .run_success(&["create", "Ticket 3"])
        .trim()
        .to_string();

    // Add ticket1 and ticket3
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket3]);

    // Add ticket2 at position 2 (between ticket1 and ticket3)
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket2, "--position", "2"]);

    // Verify order in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    let pos1 = output.find(&ticket1).unwrap();
    let pos2 = output.find(&ticket2).unwrap();
    let pos3 = output.find(&ticket3).unwrap();
    assert!(pos1 < pos2);
    assert!(pos2 < pos3);
}

#[test]
fn test_plan_remove_ticket() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create and add a ticket
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Verify ticket is in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(output.contains(&ticket_id));

    // Remove ticket
    let output = janus.run_success(&["plan", "remove-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("Removed"));

    // Verify ticket is no longer in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(!output.contains(&ticket_id));
}

#[test]
fn test_plan_remove_ticket_not_in_plan() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket but don't add it to the plan
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Try to remove ticket (should fail)
    let output = janus.run_failure(&["plan", "remove-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("not found in plan"));
}

#[test]
fn test_plan_remove_dangling_ticket_simple() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create a ticket and add it to the plan
    let ticket_id = janus
        .run_success(&["create", "Doomed Ticket"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Delete the ticket file to create a dangling reference
    janus.delete_ticket(&ticket_id);

    // Removing the dangling ticket should still succeed
    let output = janus.run_success(&["plan", "remove-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("Removed"));
    assert!(output.contains(&ticket_id));

    // Verify ticket is no longer in plan
    let plan_content = janus.read_plan(&plan_id);
    assert!(!plan_content.contains(&ticket_id));
}

#[test]
fn test_plan_remove_dangling_ticket_phased() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Create a ticket and add it to Phase One
    let ticket_id = janus
        .run_success(&["create", "Doomed Ticket"])
        .trim()
        .to_string();
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);

    // Delete the ticket file to create a dangling reference
    janus.delete_ticket(&ticket_id);

    // Removing the dangling ticket should still succeed
    let output = janus.run_success(&["plan", "remove-ticket", &plan_id, &ticket_id]);
    assert!(output.contains("Removed"));
    assert!(output.contains(&ticket_id));

    // Verify ticket is no longer in plan
    let plan_content = janus.read_plan(&plan_id);
    assert!(!plan_content.contains(&ticket_id));
}

#[test]
fn test_plan_remove_dangling_ticket_invalid_id_format() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Try to remove a ticket with an invalid ID format (no hyphen)
    let output = janus.run_failure(&["plan", "remove-ticket", &plan_id, "nohyphen"]);
    assert!(output.contains("invalid ticket ID format"));
}

#[test]
fn test_plan_remove_dangling_ticket_not_in_plan() {
    let janus = JanusTest::new();

    // Create a simple plan with a ticket
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    let ticket_id = janus
        .run_success(&["create", "Real Ticket"])
        .trim()
        .to_string();
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket_id]);

    // Try to remove a non-existent ticket that was never in the plan
    // (valid ID format but doesn't exist and isn't in the plan)
    let output = janus.run_failure(&["plan", "remove-ticket", &plan_id, "j-dead"]);
    assert!(output.contains("not found in plan"));
}

#[test]
fn test_plan_move_ticket() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Create and add a ticket to Phase One
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);

    // Move ticket to Phase Two
    let output = janus.run_success(&[
        "plan",
        "move-ticket",
        &plan_id,
        &ticket_id,
        "--to-phase",
        "Phase Two",
    ]);
    assert!(output.contains("Moved"));
    assert!(output.contains("Phase Two"));

    // Verify the move using CLI output - ticket should be after Phase 2 header
    let output = janus.run_success(&["plan", "show", &plan_id]);
    let phase2_pos = output.find("Phase 2").unwrap();
    let ticket_pos = output.rfind(&ticket_id).unwrap(); // Use rfind to find the last occurrence
    assert!(
        ticket_pos > phase2_pos,
        "Ticket should be after Phase 2 header"
    );
}

#[test]
fn test_plan_add_phase() {
    let janus = JanusTest::new();

    // Create a simple plan (no phases)
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Add a phase
    let output = janus.run_success(&["plan", "add-phase", &plan_id, "New Phase"]);
    assert!(output.contains("Added phase"));
    assert!(output.contains("New Phase"));

    // Verify phase is in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(output.contains("New Phase"));
    assert!(output.contains("## Phase"));
}

#[test]
fn test_plan_add_phase_to_phased_plan() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Add another phase
    janus.run_success(&["plan", "add-phase", &plan_id, "Phase Two"]);

    // Verify both phases are in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(output.contains("Phase One"));
    assert!(output.contains("Phase Two"));
}

#[test]
fn test_plan_remove_phase_empty() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Remove Phase One (empty, should succeed)
    let output = janus.run_success(&["plan", "remove-phase", &plan_id, "Phase One"]);
    assert!(output.contains("Removed"));

    // Verify phase is no longer in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(!output.contains("Phase One"));
    assert!(output.contains("Phase Two"));
}

#[test]
fn test_plan_remove_phase_with_force() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&["plan", "create", "Phased Plan", "--phase", "Phase One"])
        .trim()
        .to_string();

    // Create and add a ticket to the phase
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);

    // Remove phase with --force
    let output = janus.run_success(&["plan", "remove-phase", &plan_id, "Phase One", "--force"]);
    assert!(output.contains("Removed"));

    // Verify phase is gone using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(!output.contains("Phase One"));
}

#[test]
fn test_plan_remove_phase_with_migrate() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Create and add a ticket to Phase One
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket_id,
        "--phase",
        "Phase One",
    ]);

    // Remove Phase One with --migrate to Phase Two
    let output = janus.run_success(&[
        "plan",
        "remove-phase",
        &plan_id,
        "Phase One",
        "--migrate",
        "Phase Two",
    ]);
    assert!(output.contains("Migrated"));
    assert!(output.contains("Removed"));

    // Verify ticket is now in Phase Two using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    assert!(!output.contains("Phase One"));
    assert!(output.contains("Phase Two"));
    assert!(output.contains(&ticket_id));
}

#[test]
fn test_plan_add_ticket_with_after() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Create three tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    let ticket3 = janus
        .run_success(&["create", "Ticket 3"])
        .trim()
        .to_string();

    // Add ticket1 and ticket3
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket1]);
    janus.run_success(&["plan", "add-ticket", &plan_id, &ticket3]);

    // Add ticket2 after ticket1
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--after",
        &ticket1,
    ]);

    // Verify order in plan using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    let pos1 = output.find(&ticket1).unwrap();
    let pos2 = output.find(&ticket2).unwrap();
    let pos3 = output.find(&ticket3).unwrap();
    assert!(pos1 < pos2);
    assert!(pos2 < pos3);
}

#[test]
fn test_plan_not_found_for_manipulation() {
    let janus = JanusTest::new();

    // Create a ticket for add-ticket test
    let ticket_id = janus
        .run_success(&["create", "Test Ticket"])
        .trim()
        .to_string();

    // Try to add ticket to non-existent plan
    let output = janus.run_failure(&["plan", "add-ticket", "nonexistent", &ticket_id]);
    assert!(output.contains("not found"));
}

#[test]
fn test_plan_ticket_not_found() {
    let janus = JanusTest::new();

    // Create a simple plan
    let plan_id = janus
        .run_success(&["plan", "create", "Simple Plan"])
        .trim()
        .to_string();

    // Try to add non-existent ticket
    let output = janus.run_failure(&["plan", "add-ticket", &plan_id, "nonexistent-ticket"]);
    assert!(output.contains("not found"));
}

// ============================================================================
// Plan Next command tests
// ============================================================================

#[test]
fn test_plan_add_phase_with_position() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "First",
            "--phase",
            "Third",
        ])
        .trim()
        .to_string();

    // Add a phase at position 2 (between First and Third)
    janus.run_success(&["plan", "add-phase", &plan_id, "Second", "--position", "2"]);

    let output = janus.run_success(&["plan", "show", &plan_id]);
    let first_pos = output.find("First").unwrap();
    let second_pos = output.find("Second").unwrap();
    let third_pos = output.find("Third").unwrap();

    assert!(first_pos < second_pos, "First should come before Second");
    assert!(second_pos < third_pos, "Second should come before Third");
}

#[test]
fn test_plan_add_phase_with_after() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "First",
            "--phase",
            "Third",
        ])
        .trim()
        .to_string();

    // Add a phase after First
    janus.run_success(&["plan", "add-phase", &plan_id, "Second", "--after", "First"]);

    let output = janus.run_success(&["plan", "show", &plan_id]);
    let first_pos = output.find("First").unwrap();
    let second_pos = output.find("Second").unwrap();
    let third_pos = output.find("Third").unwrap();

    assert!(first_pos < second_pos, "First should come before Second");
    assert!(second_pos < third_pos, "Second should come before Third");
}

#[test]
fn test_plan_move_ticket_with_position() {
    let janus = JanusTest::new();

    // Create a phased plan
    let plan_id = janus
        .run_success(&[
            "plan",
            "create",
            "Phased Plan",
            "--phase",
            "Phase One",
            "--phase",
            "Phase Two",
        ])
        .trim()
        .to_string();

    // Create tickets
    let ticket1 = janus
        .run_success(&["create", "Ticket 1"])
        .trim()
        .to_string();
    let ticket2 = janus
        .run_success(&["create", "Ticket 2"])
        .trim()
        .to_string();
    let ticket3 = janus
        .run_success(&["create", "Ticket 3"])
        .trim()
        .to_string();

    // Add tickets to Phase One
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket1,
        "--phase",
        "Phase One",
    ]);

    // Add tickets to Phase Two
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket2,
        "--phase",
        "Phase Two",
    ]);
    janus.run_success(&[
        "plan",
        "add-ticket",
        &plan_id,
        &ticket3,
        "--phase",
        "Phase Two",
    ]);

    // Move ticket1 to Phase Two at position 1
    let output = janus.run_success(&[
        "plan",
        "move-ticket",
        &plan_id,
        &ticket1,
        "--to-phase",
        "Phase Two",
        "--position",
        "1",
    ]);
    assert!(output.contains("Moved"));

    // Verify ticket1 is now first in Phase Two using CLI output
    let output = janus.run_success(&["plan", "show", &plan_id]);
    let phase2_pos = output.find("Phase 2").unwrap();
    let t1_after_p2 = output[phase2_pos..].find(&ticket1);
    let t2_after_p2 = output[phase2_pos..].find(&ticket2);

    assert!(
        t1_after_p2.is_some() && t2_after_p2.is_some(),
        "Both tickets should be in Phase 2"
    );
    assert!(
        t1_after_p2.unwrap() < t2_after_p2.unwrap(),
        "Ticket1 should be before Ticket2 in Phase 2"
    );
}
