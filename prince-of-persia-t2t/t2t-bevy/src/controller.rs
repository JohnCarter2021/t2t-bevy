//! Character controller: gravity, ground following, jumping —
//! stepped against the collision soup. Jade units
//! (~100 per metre), Z-up. Wall collision is M6.2; this slice
//! covers vertical physics + ground snapping.

use crate::collision::TriangleSoup;

/// Gravity in Jade units/s^2 (9.8 m/s^2 at 100 units/m).
pub const GRAVITY: f32 = 980.0;
/// How far above the feet a surface may be and still count as
/// ground when falling onto it / stepping up.
pub const STEP_UP: f32 = 30.0;
/// How far below the feet ground may drop before we count as
/// airborne while walking.
pub const STEP_DOWN: f32 = 45.0;
/// Capsule radius (XY) for wall collision.
pub const RADIUS: f32 = 25.0;
/// Capsule height for wall collision.
pub const HEIGHT: f32 = 175.0;

/// While hanging, the feet sit this far below the ledge top.
pub const HANG_DROP: f32 = 178.0;
/// Hand height above the feet used for ledge grabs.
pub const HAND_HEIGHT: f32 = 170.0;
/// Seconds a climb-up transition takes.
pub const CLIMB_TIME: f32 = 0.45;

/// Body offset from a pole's center while gripping it.
pub const POLE_OFF: f32 = 38.0;
/// Pole climb speed (Jade units/s).
pub const POLE_CLIMB: f32 = 95.0;
/// Pole orbit speed (rad/s).
pub const POLE_ORBIT: f32 = 1.9;

#[derive(Debug, Clone, Copy, Default)]
pub struct MoveInput {
    /// Desired horizontal velocity (Jade units/s), XY plane.
    pub vel_xy: [f32; 2],
    pub jump: bool,
    /// Let go of a ledge (hang state only).
    pub drop: bool,
    /// Pole controls: [climb (-1..1), orbit (-1..1)] —
    /// read only while gripping a pole.
    pub pole_move: [f32; 2],
}

/// Pole-grip state: the character clings to the column at
/// `center`, positioned at `angle` around it.
#[derive(Debug, Clone, Copy)]
pub struct PoleState {
    pub center: [f32; 2],
    pub top: f32,
    pub bottom: f32,
    /// Angle around the column (radians, Jade XY plane).
    pub angle: f32,
}

/// Ledge-hang state: the character grips `top`'s edge,
/// facing into the wall (opposite `normal`).
#[derive(Debug, Clone, Copy)]
pub struct HangState {
    /// Horizontal unit normal, wall -> open air.
    pub normal: [f32; 2],
    /// Ledge top point (on the floor, inside the edge).
    pub top: [f32; 3],
    /// Climb-up progress in seconds; 0 = just hanging.
    pub climbing: f32,
}

/// Beam-walk state: locked to a narrow support's axis.
#[derive(Debug, Clone, Copy)]
pub struct BeamState {
    /// Unit vector along the beam.
    pub axis: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct Controller {
    /// Feet position in Jade world space.
    pub pos: [f32; 3],
    pub vel_z: f32,
    /// Facing angle (radians, Jade XY plane).
    pub yaw: f32,
    pub grounded: bool,
    pub hanging: Option<HangState>,
    pub beam: Option<BeamState>,
    pub pole: Option<PoleState>,
    /// Seconds before a pole can be re-grabbed after exit.
    pub pole_cooldown: f32,
    /// Throttle for the (expensive) pole search.
    pub pole_scan: f32,
}

impl Controller {
    pub fn new(pos: [f32; 3]) -> Controller {
        Controller {
            pos,
            vel_z: 0.0,
            yaw: 0.0,
            grounded: false,
            hanging: None,
            beam: None,
            pole: None,
            pole_cooldown: 0.0,
            pole_scan: 0.0,
        }
    }

    /// Snap into the hang pose for a found ledge.
    fn enter_hang(&mut self, ledge: crate::collision::Ledge) {
        self.pos = [
            ledge.top[0] + ledge.normal[0] * 10.0,
            ledge.top[1] + ledge.normal[1] * 10.0,
            ledge.top[2] - HANG_DROP,
        ];
        self.vel_z = 0.0;
        self.grounded = false;
        self.yaw = (-ledge.normal[1]).atan2(-ledge.normal[0]);
        self.hanging = Some(HangState {
            normal: ledge.normal,
            top: ledge.top,
            climbing: 0.0,
        });
    }

    pub fn step(&mut self, soup: &TriangleSoup, input: &MoveInput, dt: f32) {
        self.pole_cooldown = (self.pole_cooldown - dt).max(0.0);
        self.pole_scan = (self.pole_scan - dt).max(0.0);
        // ---- Pole state ----
        if let Some(mut pole) = self.pole {
            if input.drop {
                self.pole = None;
                self.pole_cooldown = 0.35;
                self.grounded = false;
                self.vel_z = 0.0;
                // fall through to airborne physics below
            } else if input.jump {
                // Leap off, away from the column.
                let radial = [pole.angle.cos(), pole.angle.sin()];
                self.pos[0] += radial[0] * 30.0;
                self.pos[1] += radial[1] * 30.0;
                self.pole = None;
                self.pole_cooldown = 0.35;
                self.grounded = false;
                self.vel_z = 420.0;
                // fall through to airborne physics below
            } else {
                pole.angle += input.pole_move[1] * POLE_ORBIT * dt;
                let mut z = self.pos[2] + input.pole_move[0] * POLE_CLIMB * dt;
                z = z.clamp(pole.bottom + 2.0, (pole.top - 165.0).max(pole.bottom + 2.0));
                let radial = [pole.angle.cos(), pole.angle.sin()];
                self.pos = [
                    pole.center[0] + radial[0] * POLE_OFF,
                    pole.center[1] + radial[1] * POLE_OFF,
                    z,
                ];
                self.yaw = (pole.angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
                self.vel_z = 0.0;
                // Stepping off at the base: the floor is
                // right under the feet.
                if input.pole_move[0] < -0.01 {
                    if let Some(g) = soup.ground_height(self.pos[0], self.pos[1], self.pos[2], STEP_UP) {
                        if self.pos[2] <= g + 8.0 {
                            self.pos[2] = g;
                            self.pole = None;
                            self.grounded = true;
                            return;
                        }
                    }
                }
                self.pole = Some(pole);
                return;
            }
        }
        // ---- Hang state ----
        if let Some(mut hang) = self.hanging {
            if hang.climbing > 0.0 {
                // Climb-up transition: frozen grip, then land
                // on top, inside the edge.
                hang.climbing += dt;
                if hang.climbing >= CLIMB_TIME {
                    self.pos = [
                        hang.top[0] - hang.normal[0] * (RADIUS * 0.8),
                        hang.top[1] - hang.normal[1] * (RADIUS * 0.8),
                        hang.top[2],
                    ];
                    self.hanging = None;
                    self.grounded = true;
                    self.vel_z = 0.0;
                } else {
                    self.hanging = Some(hang);
                }
                return;
            }
            if input.drop {
                self.hanging = None;
                self.grounded = false;
                self.vel_z = 0.0;
                // fall through to airborne physics below
            } else if input.jump {
                hang.climbing = f32::EPSILON;
                self.hanging = Some(hang);
                return;
            } else {
                // Shimmy: project desired velocity on the
                // wall tangent, then re-snap to the ledge.
                let tangent = [-hang.normal[1], hang.normal[0]];
                let along = input.vel_xy[0] * tangent[0] + input.vel_xy[1] * tangent[1];
                if along.abs() > 0.01 {
                    let cand = [
                        self.pos[0] + tangent[0] * along * dt,
                        self.pos[1] + tangent[1] * along * dt,
                        self.pos[2],
                    ];
                    let face = [-hang.normal[0], -hang.normal[1]];
                    if let Some(l) = soup.find_ledge(cand, face, self.pos[2] + HANG_DROP) {
                        if (l.top[2] - hang.top[2]).abs() < 30.0 {
                            self.enter_hang(l);
                            // enter_hang resets climbing; fine.
                            return;
                        }
                    }
                    // Ledge ended or changed height: stay put.
                    return;
                }
                return; // hanging still: no gravity, no drift
            }
        }
        // Horizontal motion, then wall resolution (slide).
        self.pos[0] += input.vel_xy[0] * dt;
        self.pos[1] += input.vel_xy[1] * dt;
        let resolved = soup.resolve_walls(
            [self.pos[0], self.pos[1]],
            RADIUS,
            self.pos[2],
            HEIGHT,
        );
        self.pos[0] = resolved[0];
        self.pos[1] = resolved[1];
        if input.vel_xy[0].abs() > 0.01 || input.vel_xy[1].abs() > 0.01 {
            self.yaw = input.vel_xy[1].atan2(input.vel_xy[0]);
        }

        // Pole grab: airborne, near a column. The search
        // radius is wider than the grab distance so the
        // whole column is in range — a partial view biases
        // the axis estimate. The search is throttled to
        // ~7 Hz: it ray-probes the local geometry.
        if !self.grounded
            && self.hanging.is_none()
            && self.pole.is_none()
            && self.pole_cooldown <= 0.0
            && self.pole_scan <= 0.0
        {
            self.pole_scan = 0.15;
            if let Some(p) = soup.find_pole(self.pos, 175.0) {
                let dx = self.pos[0] - p.center[0];
                let dy = self.pos[1] - p.center[1];
                let d = (dx * dx + dy * dy).sqrt();
                if d <= 85.0 && self.pos[2] < p.top - 120.0 && self.pos[2] > p.bottom - 40.0 {
                    let angle = dy.atan2(dx);
                    let radial = [angle.cos(), angle.sin()];
                    self.pos = [
                        p.center[0] + radial[0] * POLE_OFF,
                        p.center[1] + radial[1] * POLE_OFF,
                        self.pos[2].clamp(p.bottom + 5.0, (p.top - 165.0).max(p.bottom + 5.0)),
                    ];
                    self.yaw = (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
                    self.vel_z = 0.0;
                    self.pole = Some(PoleState {
                        center: p.center,
                        top: p.top,
                        bottom: p.bottom,
                        angle,
                    });
                    return;
                }
            }
        }

        // Ledge grab: airborne, not rocketing upward, with a
        // ledge in the hand band in the facing/move direction.
        if !self.grounded && self.hanging.is_none() && self.vel_z <= 200.0 {
            let dir = if input.vel_xy[0].abs() > 0.01 || input.vel_xy[1].abs() > 0.01 {
                let l = (input.vel_xy[0].powi(2) + input.vel_xy[1].powi(2)).sqrt();
                [input.vel_xy[0] / l, input.vel_xy[1] / l]
            } else {
                [self.yaw.cos(), self.yaw.sin()]
            };
            if let Some(l) = soup.find_ledge(self.pos, dir, self.pos[2] + HAND_HEIGHT) {
                self.enter_hang(l);
                return;
            }
        }

        if self.grounded && input.jump {
            self.vel_z = 420.0; // ~0.9 m jump
            self.grounded = false;
        }

        // Beam lock: on a narrow support, snap to its
        // centerline (lateral input is absorbed); the state
        // clears when the support widens or ends.
        if self.grounded {
            let perp = [-self.yaw.sin(), self.yaw.cos()];
            match soup.beam_span(self.pos, perp) {
                Some(span) => {
                    let rel = [
                        self.pos[0] - span.center[0],
                        self.pos[1] - span.center[1],
                    ];
                    let lat = rel[0] * perp[0] + rel[1] * perp[1];
                    self.pos[0] -= perp[0] * lat;
                    self.pos[1] -= perp[1] * lat;
                    self.beam = Some(BeamState { axis: span.axis });
                }
                None => self.beam = None,
            }
        } else {
            self.beam = None;
        }

        if self.grounded {
            // Follow the ground; walk off an edge -> airborne.
            match soup.ground_height(self.pos[0], self.pos[1], self.pos[2], STEP_UP) {
                Some(g) if g >= self.pos[2] - STEP_DOWN => {
                    self.pos[2] = g;
                    self.vel_z = 0.0;
                }
                _ => {
                    self.grounded = false;
                    self.vel_z = 0.0;
                }
            }
        }

        if !self.grounded {
            self.vel_z -= GRAVITY * dt;
            self.pos[2] += self.vel_z * dt;
            if self.vel_z <= 0.0 {
                if let Some(g) = soup.ground_height(self.pos[0], self.pos[1], self.pos[2], STEP_UP) {
                    if self.pos[2] <= g + 1.0 {
                        self.pos[2] = g;
                        self.vel_z = 0.0;
                        self.grounded = true;
                    }
                }
            }
        }
    }
}

/// Checkpoint tracking: the last safe grounded spot the
/// player reached, used as the respawn point. A new
/// checkpoint is recorded when the player is grounded
/// at least REBASE_DIST (XY) from the current one — so
/// progress through a level keeps pulling the respawn
/// point forward, while small movements (and anything
/// airborne) never touch it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    pub point: Option<[f32; 3]>,
    pub yaw: f32,
}

impl Checkpoint {
    /// Distance (XY, Jade units) that re-bases the
    /// checkpoint.
    pub const REBASE_DIST: f32 = 400.0;

    pub fn new() -> Checkpoint {
        Checkpoint { point: None, yaw: 0.0 }
    }

    /// Seed a checkpoint directly (level-placed points,
    /// staging).
    pub fn seed(&mut self, pos: [f32; 3], yaw: f32) {
        self.point = Some(pos);
        self.yaw = yaw;
    }

    /// Observe the player for this frame; returns true
    /// when a NEW checkpoint was recorded.
    pub fn observe(&mut self, pos: [f32; 3], yaw: f32, grounded: bool) -> bool {
        if !grounded {
            return false;
        }
        let far = match self.point {
            None => true,
            Some(p) => {
                let dx = pos[0] - p[0];
                let dy = pos[1] - p[1];
                dx * dx + dy * dy >= Self::REBASE_DIST * Self::REBASE_DIST
            }
        };
        if far {
            self.point = Some(pos);
            self.yaw = yaw;
            true
        } else {
            false
        }
    }

    /// Where a respawn goes: the checkpoint if one is
    /// recorded, else the level spawn.
    pub fn respawn_point(&self, fallback_spawn: [f32; 3]) -> [f32; 3] {
        self.point.unwrap_or(fallback_spawn)
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    #[test]
    fn first_grounded_spot_becomes_the_checkpoint() {
        let mut c = Checkpoint::new();
        assert!(c.observe([10.0, 20.0, 60.0], 1.0, true));
        assert_eq!(c.respawn_point([0.0; 3]), [10.0, 20.0, 60.0]);
        assert_eq!(c.yaw, 1.0);
    }

    #[test]
    fn airborne_and_small_moves_never_rebase() {
        let mut c = Checkpoint::new();
        assert!(c.observe([0.0, 0.0, 60.0], 0.0, true));
        // Airborne far away: ignored.
        assert!(!c.observe([900.0, 0.0, 300.0], 0.0, false));
        // Grounded but only 399 away: ignored.
        assert!(!c.observe([399.0, 0.0, 60.0], 0.0, true));
        assert_eq!(c.respawn_point([9.0; 3]), [0.0, 0.0, 60.0]);
        // Grounded 400+ away: re-based.
        assert!(c.observe([400.0, 0.0, 60.0], 2.0, true));
        assert_eq!(c.respawn_point([9.0; 3]), [400.0, 0.0, 60.0]);
    }

    #[test]
    fn no_checkpoint_falls_back_to_spawn() {
        let c = Checkpoint::new();
        assert_eq!(c.respawn_point([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }
}
