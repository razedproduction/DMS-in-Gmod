//! Homer's look: his ped's meshes (`m_y_pmanhat_01.wdd`, skinned to the fragment's skeleton) reskinned onto NM's
//! 21 parts and moved from the skeleton's bind pose into NM's standing pose.
//!
//! A game bone goes to the part on it or on its nearest ancestor (fingers to the hand, roll bones to their limb);
//! each part carries its bones rigidly from the bind pose: `M = F * offset^-1 * B^-1` (`F` the part's standing
//! frame, `offset` the build's bone-to-part transform, `B` the part's bone in the bind pose).

use delirium::prelude::*;
use delirium::ragdoll::{Build, pose_frames};
use delirium_data::skeleton;

use crate::mesh::Mesh;
use crate::smd;

/// The components drawn: variation 000 of the head, the upper and the lower body (their diffuse textures' `_a_`).
pub const COMPONENTS: [&str; 3] = ["head_diff_000_a", "uppr_diff_000_a", "lowr_diff_000_a"];

/// For every game bone, the part it's reskinned to.
pub fn part_of_bones(build: &Build, bones: &[skeleton::Bone]) -> Vec<usize> {
    (0..bones.len())
        .map(|b| {
            let mut at = Some(b);
            while let Some(i) = at {
                if let Some(k) = build.parts.iter().position(|p| p.bone == i) {
                    return k;
                }
                at = bones[i].parent;
            }
            0
        })
        .collect()
}

/// Each part's move from the bind pose to `placed` (its standing frames, model space).
fn moves(build: &Build, bones: &[skeleton::Bone], placed: &[(Vec3, Quat)]) -> Vec<Mat4> {
    let parents: Vec<Option<usize>> = bones.iter().map(|b| b.parent).collect();
    let bind: Vec<(Vec3, Quat)> = bones.iter().map(|b| (Vec3::from(b.translation), Quat::from_array(b.rotation).normalize())).collect();
    let world = pose_frames(&parents, &bind, Transform::IDENTITY);
    build.parts.iter().zip(placed).map(|(p, &(pos, rot))| {
        let (bt, br) = world[p.bone];
        let f = Mat4::from_rotation_translation(rot, pos);
        f * p.offset.to_matrix().inverse() * Mat4::from_rotation_translation(br, bt).inverse()
    }).collect()
}

/// The meshes as SMD triangles on NM's bones in `placed`; each mesh's material its texture's name.
pub fn triangles(build: &Build, bones: &[skeleton::Bone], placed: &[(Vec3, Quat)], meshes: &[Mesh]) -> Vec<smd::Triangle> {
    let part_of = part_of_bones(build, bones);
    let m = moves(build, bones, placed);
    let mut out = vec![];
    for mesh in meshes {
        let material = mesh.texture.clone().unwrap_or_else(|| "homer_capsule".into());
        let verts: Vec<smd::Vertex> = (0..mesh.positions.len()).map(|v| {
            let p = Vec3::from(mesh.positions[v]);
            let n = mesh.normals.get(v).map_or(Vec3::Z, |n| Vec3::from(*n));
            let mut weights: Vec<(usize, f32)> = vec![];
            for &(b, w) in mesh.skin.get(v).map_or(&[][..], |s| &s[..]) {
                let k = part_of.get(b).copied().unwrap_or(0);
                match weights.iter_mut().find(|x| x.0 == k) {
                    Some(x) => x.1 += w,
                    None => weights.push((k, w)),
                }
            }
            if weights.is_empty() {
                weights.push((0, 1.0));
            }
            let total: f32 = weights.iter().map(|w| w.1).sum();
            weights.iter_mut().for_each(|w| w.1 /= total);
            let pos = weights.iter().map(|&(k, w)| m[k].transform_point3(p) * w).sum::<Vec3>();
            let normal = weights.iter().map(|&(k, w)| m[k].transform_vector3(n) * w).sum::<Vec3>().normalize_or(Vec3::Z);
            let uv = mesh.uvs.get(v).copied().unwrap_or([0.0, 0.0]);
            smd::Vertex { pos, normal, uv: [uv[0], 1.0 - uv[1]], weights }
        }).collect();
        // The game's front faces are clockwise (Direct3D's), SMD's counter-clockwise: keep whichever winding
        // agrees with the vertex normals.
        let facing: f32 = mesh.indices.chunks(3).filter(|t| t.len() == 3).map(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| &verts[i as usize]);
            (b.pos - a.pos).cross(c.pos - a.pos).dot(a.normal + b.normal + c.normal)
        }).sum();
        for t in mesh.indices.chunks(3).filter(|t| t.len() == 3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| verts[i as usize].clone());
            let v = if facing >= 0.0 { [a, b, c] } else { [a, c, b] };
            out.push(smd::Triangle { material: material.clone(), v });
        }
    }
    out
}
