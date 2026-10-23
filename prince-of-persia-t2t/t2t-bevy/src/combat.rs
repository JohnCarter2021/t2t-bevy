//! Combat state: hit points, attack timing windows, and
//! hit resolution (range + facing arc). Pure logic — the
//! game layer owns animation and movement; this module
//! decides who gets hit, when, and how hard.

/// What a fighter is doing, combat-wise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FighterState {
    Ready,
    /// Swinging; `t` = seconds since the swing started,
    /// `hit_done` = the strike has already connected once.
    Attacking { t: f32, hit_done: bool },
    /// Staggered by a hit; `t` = seconds since it began.
    HitReact { t: f32 },
    Dead,
}

/// An attack's shape: when during the swing it can connect,
/// and the reach/arc it covers.
#[derive(Debug, Clone, Copy)]
pub struct StrikeProfile {
    /// Total swing duration (s).
    pub duration: f32,
    /// Active window start/end (s within the swing).
    pub active: (f32, f32),
    /// Reach in Jade units (XY distance).
    pub range: f32,
    /// Half-angle of the facing arc (radians).
    pub half_angle: f32,
    pub damage: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct Fighter {
    pub hp: i32,
    pub max_hp: i32,
    pub state: FighterState,
    /// Seconds until the next swing is allowed.
    pub cooldown: f32,
    pub strike: StrikeProfile,
}

impl Fighter {
    pub fn new(max_hp: i32, strike: StrikeProfile) -> Fighter {
        Fighter { hp: max_hp, max_hp, state: FighterState::Ready, cooldown: 0.0, strike }
    }

    pub fn alive(&self) -> bool {
        self.state != FighterState::Dead
    }

    /// Advance timers; returns true when a HitReact ends.
    pub fn update(&mut self, dt: f32, hitreact_duration: f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        match &mut self.state {
            FighterState::Attacking { t, .. } => {
                *t += dt;
                if *t >= self.strike.duration {
                    self.state = FighterState::Ready;
                }
            }
            FighterState::HitReact { t } => {
                *t += dt;
                if *t >= hitreact_duration {
                    self.state = FighterState::Ready;
                }
            }
            _ => {}
        }
    }

    /// Begin a swing if allowed. Returns whether it started.
    pub fn start_attack(&mut self, cooldown_after: f32) -> bool {
        if self.state == FighterState::Ready && self.cooldown <= 0.0 {
            self.state = FighterState::Attacking { t: 0.0, hit_done: false };
            self.cooldown = cooldown_after;
            true
        } else {
            false
        }
    }

    /// Take a hit: applies damage, interrupts into HitReact
    /// (or Dead). Returns true if the hit landed.
    pub fn take_hit(&mut self, damage: i32) -> bool {
        if !self.alive() {
            return false;
        }
        self.hp -= damage;
        if self.hp <= 0 {
            self.hp = 0;
            self.state = FighterState::Dead;
        } else {
            self.state = FighterState::HitReact { t: 0.0 };
        }
        true
    }

    /// If this fighter is mid-swing inside the active
    /// window and the target is in range + arc, connect
    /// the strike (once per swing). `from`/`to` are Jade
    /// positions; `yaw` is the attacker's facing.
    pub fn strike_connects(
        &mut self,
        from: [f32; 3],
        yaw: f32,
        to: [f32; 3],
        target: &mut Fighter,
    ) -> bool {
        let (t, hit_done) = match &mut self.state {
            FighterState::Attacking { t, hit_done } => (*t, hit_done),
            _ => return false,
        };
        if *hit_done || t < self.strike.active.0 || t > self.strike.active.1 {
            return false;
        }
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > self.strike.range || dist < 1e-6 {
            return false;
        }
        // Fighters must share a floor, roughly: no hitting
        // through balconies or across drops.
        if (to[2] - from[2]).abs() > 200.0 {
            return false;
        }
        // Facing arc: angle between facing and the target
        // direction within the half-angle.
        let ang = dy.atan2(dx);
        let mut diff = (ang - yaw).rem_euclid(std::f32::consts::TAU);
        if diff > std::f32::consts::PI {
            diff -= std::f32::consts::TAU;
        }
        if diff.abs() > self.strike.half_angle {
            return false;
        }
        if let FighterState::Attacking { hit_done, .. } = &mut self.state {
            *hit_done = true;
        }
        target.take_hit(self.strike.damage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> StrikeProfile {
        StrikeProfile {
            duration: 0.3,
            active: (0.1, 0.25),
            range: 240.0,
            half_angle: 1.1,
            damage: 1,
        }
    }

    #[test]
    fn strike_hits_once_in_window_range_and_arc() {
        let mut a = Fighter::new(5, profile());
        let mut b = Fighter::new(3, profile());
        assert!(a.start_attack(0.8));
        // Too early: no connect.
        assert!(!a.strike_connects([0.0, 0.0, 0.0], 0.0, [150.0, 0.0, 0.0], &mut b));
        a.update(0.15, 0.5);
        // In window, in range, dead ahead: connects.
        assert!(a.strike_connects([0.0, 0.0, 0.0], 0.0, [150.0, 0.0, 0.0], &mut b));
        assert_eq!(b.hp, 2);
        assert!(matches!(b.state, FighterState::HitReact { .. }));
        // Same swing cannot connect twice.
        assert!(!a.strike_connects([0.0, 0.0, 0.0], 0.0, [150.0, 0.0, 0.0], &mut b));
        assert_eq!(b.hp, 2);
    }

    #[test]
    fn strike_misses_out_of_range_and_behind() {
        let mut a = Fighter::new(5, profile());
        let mut b = Fighter::new(3, profile());
        a.start_attack(0.8);
        a.update(0.15, 0.5);
        assert!(!a.strike_connects([0.0, 0.0, 0.0], 0.0, [900.0, 0.0, 0.0], &mut b));
        assert!(!a.strike_connects([0.0, 0.0, 0.0], 0.0, [-150.0, 0.0, 0.0], &mut b));
        assert_eq!(b.hp, 3);
    }

    #[test]
    fn death_at_zero_and_hitreact_recovery() {
        let mut b = Fighter::new(1, profile());
        assert!(b.take_hit(1));
        assert_eq!(b.state, FighterState::Dead);
        assert!(!b.take_hit(1), "the dead stay dead");
        let mut c = Fighter::new(3, profile());
        c.take_hit(1);
        c.update(0.3, 0.5);
        assert!(matches!(c.state, FighterState::HitReact { .. }));
        c.update(0.3, 0.5);
        assert_eq!(c.state, FighterState::Ready);
    }
}

/// The Prince's sand: gained by defeating enemies,
/// spent on time powers. Pure bookkeeping — the game
/// layer owns effects and timing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SandTank {
    pub sand: u32,
    pub max: u32,
}

impl SandTank {
    pub fn new(max: u32, start: u32) -> SandTank {
        SandTank { sand: start.min(max), max }
    }

    /// Absorb `n` sand; returns how much actually fit.
    pub fn gain(&mut self, n: u32) -> u32 {
        let got = n.min(self.max - self.sand);
        self.sand += got;
        got
    }

    /// Spend `n` sand if the tank holds it.
    pub fn spend(&mut self, n: u32) -> bool {
        if self.sand >= n {
            self.sand -= n;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod sand_tests {
    use super::*;

    #[test]
    fn gain_caps_at_max() {
        let mut t = SandTank::new(6, 5);
        assert_eq!(t.gain(2), 1);
        assert_eq!(t.sand, 6);
        assert_eq!(t.gain(1), 0);
    }

    #[test]
    fn spend_needs_the_sand() {
        let mut t = SandTank::new(6, 1);
        assert!(!t.spend(2));
        assert_eq!(t.sand, 1);
        assert!(t.spend(1));
        assert_eq!(t.sand, 0);
        assert!(!t.spend(1));
    }

    #[test]
    fn start_clamps_to_max() {
        assert_eq!(SandTank::new(6, 99).sand, 6);
    }
}
