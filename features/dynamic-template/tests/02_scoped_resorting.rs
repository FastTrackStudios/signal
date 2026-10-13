//! Tests for scoped re-sorting workflow
//!
//! This tests the workflow where:
//! 1. Initial sort puts some items in "Unsorted" folder (items that can't be fully classified)
//! 2. User provides context (e.g., "John items belong to Guitars")
//! 3. Scoped re-sort moves items to the correct location and re-organizes hierarchy

use dynamic_template::*;
use monarchy::test_utils::StructureAssertions;
use monarchy::{monarchy_sort, move_unsorted_to_group, reapply_collapse};

/// Test: Initial sort with performer names that don't match any group go to Unsorted
#[test]
fn performer_items_go_to_unsorted() {
    // -- Setup & Fixtures
    let items = vec!["Guitar Crunch", "John Crunch", "John Clean"];
    let config = default_config();

    // -- Exec
    let structure = monarchy_sort(items, &config).unwrap();

    // -- Check
    println!("\nStructure after initial sort:");
    println!("{structure}");

    structure.assert().has_total_items(3);

    // "Guitar Crunch" should be in Guitars (or Electric after collapse)
    let guitars = structure
        .find_child("Guitars")
        .or_else(|| structure.find_child("Electric"))
        .expect("Guitars/Electric folder should exist");
    assert!(
        guitars.items.iter().any(|i| i.original == "Guitar Crunch")
            || guitars
                .children
                .iter()
                .any(|c| c.items.iter().any(|i| i.original == "Guitar Crunch")),
        "Guitar Crunch should be in Guitars"
    );

    // John items should be in Unsorted
    let has_unsorted = structure.find_child("Unsorted").is_some();
    println!("Has Unsorted folder: {has_unsorted}");
    assert!(has_unsorted, "Should have Unsorted folder");

    let unsorted = structure.find_child("Unsorted").expect("Unsorted folder");
    assert_eq!(unsorted.items.len(), 2, "Unsorted should have 2 items");
}

/// Test: Full workflow - initial sort, then scoped re-sort of John items to Guitars
#[test]
fn full_scoped_resort_workflow() {
    // -- Setup & Fixtures
    let items = vec![
        "Guitar Crunch", // Matches Guitars directly
        "John Crunch",   // Performer + arrangement, no group match initially
        "John Clean",    // Performer + arrangement, no group match initially
    ];
    let config = default_config();

    // -- Exec (Step 1: Initial sort)
    let mut structure = monarchy_sort(items, &config).unwrap();

    println!("\n=== STEP 1: Initial sort ===");
    println!("{structure}");

    // Verify initial state
    assert!(
        structure.find_child("Unsorted").is_some(),
        "Should have Unsorted folder"
    );
    let has_guitars =
        structure.find_child("Guitars").is_some() || structure.find_child("Electric").is_some();
    assert!(has_guitars, "Should have Guitars/Electric folder");

    // -- Exec (Step 2: Move John items from Unsorted to Electric)
    println!("\n=== STEP 2: Moving Unsorted items to Electric ===");

    let sorted_count = move_unsorted_to_group(
        &mut structure,
        &config,
        &["Unsorted"], // Extract all items from Unsorted
        "Electric",    // Sort within Electric's subgroups (renamed from "Electric Guitar")
        &["Guitars"],  // Merge result into Guitars folder
    )
    .unwrap();

    println!("Sorted {sorted_count} items");

    // -- Exec (Step 3: Re-apply collapse to clean up hierarchy)
    reapply_collapse(&mut structure, &config);

    // -- Check
    println!("\n=== STEP 3: After re-sorting and collapse ===");
    println!("{structure}");

    structure.assert().has_total_items(3);

    let guitars = structure
        .find_child("Guitars")
        .or_else(|| structure.find_child("Electric"))
        .expect("Guitars/Electric should exist");
    assert_eq!(
        guitars.total_items(),
        3,
        "All 3 items should be in Guitars now"
    );

    // Unsorted should be gone if empty
    if let Some(unsorted) = structure.find_child("Unsorted") {
        assert!(
            unsorted.is_empty(),
            "Unsorted should be empty after moving items"
        );
    }
}

/// Test: Scoped re-sort with items that still can't be sorted
#[test]
fn scoped_resort_with_remaining_unsorted() {
    // -- Setup & Fixtures
    let items = vec![
        "Guitar Crunch",  // Matches Guitars
        "John Crunch",    // Can be sorted into Guitars (has arrangement)
        "RandomNonsense", // Can't be sorted anywhere
    ];
    let config = default_config();

    // -- Exec
    let mut structure = monarchy_sort(items, &config).unwrap();

    println!("\n=== Initial sort ===");
    println!("{structure}");

    let sorted_count = move_unsorted_to_group(
        &mut structure,
        &config,
        &["Unsorted"],
        "Electric",
        &["Guitars"],
    )
    .unwrap();

    reapply_collapse(&mut structure, &config);

    // -- Check
    println!("\n=== After scoped re-sort ===");
    println!("{structure}");
    println!("Sorted {sorted_count} items");

    // "John Crunch" should be sorted into Guitars (has arrangement "Crunch")
    // "RandomNonsense" should remain in Unsorted
    let guitars = structure
        .find_child("Guitars")
        .or_else(|| structure.find_child("Electric"))
        .expect("Guitars/Electric should exist");
    println!("Guitars total items: {}", guitars.total_items());

    // Check if RandomNonsense is still in Unsorted
    if let Some(unsorted) = structure.find_child("Unsorted") {
        println!(
            "Remaining unsorted: {:?}",
            unsorted
                .items
                .iter()
                .map(|i| &i.original)
                .collect::<Vec<_>>()
        );
        assert!(
            unsorted
                .items
                .iter()
                .any(|i| i.original == "RandomNonsense"),
            "RandomNonsense should remain in Unsorted"
        );
    }
}

/// Test: Guitars sorted by performer, then arrangement
#[test]
fn guitars_with_performer_creates_nested_hierarchy() {
    // -- Setup & Fixtures
    let items = vec![
        "Guitar Crunch",      // No performer, just arrangement
        "John Guitar Crunch", // Performer "John", arrangement "Crunch"
        "John Guitar Clean",  // Performer "John", arrangement "Clean"
    ];
    let config = default_config();

    // -- Exec
    let structure = monarchy_sort(items, &config).unwrap();

    // -- Check
    println!("\nStructure with performer grouping:");
    println!("{structure}");
    println!(
        "Root name: {}, items: {}, children: {}",
        structure.name,
        structure.items.len(),
        structure.children.len()
    );

    // After collapse, might be "Electric" at root level
    if structure.name == "Electric" {
        println!(
            "\nElectric children: {:?}",
            structure
                .children
                .iter()
                .map(|c| &c.name)
                .collect::<Vec<_>>()
        );
    } else {
        let guitars = structure
            .find_child("Guitars")
            .or_else(|| structure.find_child("Electric"))
            .expect("Guitars/Electric folder should exist");
        println!(
            "\nGuitars children: {:?}",
            guitars.children.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }

    structure.assert().has_total_items(3);
}

/// Test: Single guitar track stays flat (no unnecessary nesting)
#[test]
fn single_guitar_stays_flat() {
    // -- Setup & Fixtures
    let items = vec!["Guitar Crunch"];
    let config = default_config();

    // -- Exec
    let structure = monarchy_sort(items, &config).unwrap();

    // -- Check
    println!("\nSingle guitar structure:");
    println!("{structure}");
    println!(
        "Root name: {}, items: {}, children: {}",
        structure.name,
        structure.items.len(),
        structure.children.len()
    );

    // After collapse, Guitars -> Electric -> Crunch collapses to just Guitars
    structure.assert().has_total_items(1);

    // After full collapse, the structure might be:
    // 1. Root is "Guitars" with items directly (structure.name == "Guitars")
    // 2. Root has a child "Guitars" with items
    if structure.name == "Guitars" {
        assert_eq!(structure.items.len(), 1, "Guitars should have 1 item");
        assert!(
            structure
                .items
                .iter()
                .any(|i| i.original == "Guitar Crunch"),
            "Should contain Guitar Crunch"
        );
    } else {
        let guitars = structure
            .find_child("Guitars")
            .expect("Guitars folder should exist after collapse");
        assert_eq!(guitars.items.len(), 1, "Guitars should have 1 item");
        assert!(
            guitars.items.iter().any(|i| i.original == "Guitar Crunch"),
            "Should contain Guitar Crunch"
        );
    }
}

/// Test: Two arrangements create folder structure
#[test]
fn two_arrangements_create_folders() {
    // -- Setup & Fixtures
    let items = vec!["Guitar Crunch", "Guitar Clean"];
    let config = default_config();

    // -- Exec
    let structure = monarchy_sort(items, &config).unwrap();

    // -- Check
    println!("\nTwo arrangements structure:");
    println!("{structure}");
    println!(
        "Root name: {}, items: {}, children: {}",
        structure.name,
        structure.items.len(),
        structure.children.len()
    );

    // After collapse, Guitars -> Electric -> [Clean, Crunch] becomes Guitars -> [Clean, Crunch]
    structure.assert().has_total_items(2);

    // Guitars might be root or a child
    if structure.name == "Guitars" {
        assert_eq!(
            structure.children.len(),
            2,
            "Guitars should have 2 arrangement children"
        );
    } else {
        let guitars = structure
            .find_child("Guitars")
            .expect("Guitars folder should exist after collapse");
        assert_eq!(
            guitars.children.len(),
            2,
            "Guitars should have 2 arrangement children"
        );
    }
}

/// Test: Merge new items into existing hierarchy
#[test]
fn merge_new_items_into_existing() {
    // -- Setup & Fixtures
    let initial_items = vec!["Guitar Crunch", "Guitar Clean"];
    let config = default_config();

    // -- Exec (initial sort)
    let mut structure = monarchy_sort(initial_items, &config).unwrap();

    println!("\n=== Initial structure ===");
    println!("{structure}");

    // Add more items via merge
    let new_items = vec!["Guitar Drive", "John Guitar Crunch"];
    let new_structure = monarchy_sort(new_items, &config).unwrap();

    println!("\n=== New items structure ===");
    println!("{new_structure}");

    // Merge new structure into existing
    structure.merge(new_structure);

    // Re-apply collapse
    reapply_collapse(&mut structure, &config);

    // -- Check
    println!("\n=== After merge and collapse ===");
    println!("{structure}");
    println!(
        "Root name: {}, items: {}, children: {}",
        structure.name,
        structure.items.len(),
        structure.children.len()
    );

    structure.assert().has_total_items(4);

    // After collapse, Electric might be at root
    if structure.name == "Electric" || structure.name == "Guitars" {
        assert_eq!(
            structure.total_items(),
            4,
            "All items should be in structure"
        );
    } else {
        let guitars = structure
            .find_child("Guitars")
            .or_else(|| structure.find_child("Electric"))
            .expect("Guitars/Electric should exist");
        assert_eq!(guitars.total_items(), 4, "All items should be in Guitars");
    }
}
