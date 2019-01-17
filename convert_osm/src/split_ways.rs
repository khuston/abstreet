// Copyright 2018 Google LLC, licensed under http://www.apache.org/licenses/LICENSE-2.0

use crate::srtm;
use dimensioned::si;
use geom::{HashablePt2D, LonLat};
use map_model::{raw_data, IntersectionType};
use std::collections::{BTreeSet, HashMap};

pub fn split_up_roads(
    (mut roads, buildings, areas): (
        Vec<raw_data::Road>,
        Vec<raw_data::Building>,
        Vec<raw_data::Area>,
    ),
    elevation: &srtm::Elevation,
) -> raw_data::Map {
    println!("splitting up {} roads", roads.len());

    // Look for roundabout ways. Map all points on the roundabout to a new point in the center.
    // When we process ways that touch any point on the roundabout, make them instead point to the
    // roundabout's center, so that the roundabout winds up looking like a single intersection.
    let mut remap_roundabouts: HashMap<HashablePt2D, LonLat> = HashMap::new();
    roads.retain(|r| {
        if r.osm_tags.get("junction") == Some(&"roundabout".to_string()) {
            let center = LonLat::center(&r.points);
            for pt in &r.points {
                remap_roundabouts.insert(pt.to_hashable(), center);
            }
            false
        } else {
            true
        }
    });

    let mut counts_per_pt: HashMap<HashablePt2D, usize> = HashMap::new();
    let mut intersections: BTreeSet<HashablePt2D> = BTreeSet::new();
    for r in roads.iter_mut() {
        let added_to_start = if let Some(center) = remap_roundabouts.get(&r.points[0].to_hashable())
        {
            r.points.insert(0, *center);
            true
        } else {
            false
        };
        let added_to_end =
            if let Some(center) = remap_roundabouts.get(&r.points.last().unwrap().to_hashable()) {
                r.points.push(*center);
                true
            } else {
                false
            };

        for (idx, raw_pt) in r.points.iter().enumerate() {
            let pt = raw_pt.to_hashable();
            counts_per_pt.entry(pt).or_insert(0);
            let count = counts_per_pt[&pt] + 1;
            counts_per_pt.insert(pt, count);

            if count == 2 {
                intersections.insert(pt);
            }

            // All start and endpoints of ways are also intersections.
            if idx == 0 || idx == r.points.len() - 1 {
                intersections.insert(pt);
            } else if remap_roundabouts.contains_key(&pt) {
                if idx == 1 && added_to_start {
                    continue;
                }
                if idx == r.points.len() - 2 && added_to_end {
                    continue;
                }
                panic!(
                    "OSM way {} hits a roundabout not at an endpoint. idx {} of length {}",
                    r.osm_way_id,
                    idx,
                    r.points.len()
                );
            }
        }
    }

    let mut map = raw_data::Map::blank();
    map.buildings = buildings;
    map.areas = areas;

    let mut pt_to_intersection: HashMap<HashablePt2D, raw_data::StableIntersectionID> =
        HashMap::new();
    for (idx, pt) in intersections.into_iter().enumerate() {
        let id = raw_data::StableIntersectionID(idx);
        map.intersections.insert(
            id,
            raw_data::Intersection {
                point: LonLat::new(pt.x(), pt.y()),
                elevation: elevation.get(pt.x(), pt.y()) * si::M,
                intersection_type: IntersectionType::StopSign,
                label: None,
            },
        );
        pt_to_intersection.insert(pt, id);
    }

    // Now actually split up the roads based on the intersections
    for orig_road in &roads {
        let mut r = orig_road.clone();
        r.points.clear();
        r.i1 = pt_to_intersection[&orig_road.points[0].to_hashable()];

        for pt in &orig_road.points {
            r.points.push(pt.clone());
            if r.points.len() > 1 {
                if let Some(i2) = pt_to_intersection.get(&pt.to_hashable()) {
                    r.i2 = *i2;
                    // Start a new road
                    map.roads
                        .insert(raw_data::StableRoadID(map.roads.len()), r.clone());
                    r.points.clear();
                    r.i1 = *i2;
                    r.points.push(pt.clone());
                }
            }
        }
        assert!(r.points.len() == 1);
    }

    map
}
