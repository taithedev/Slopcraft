use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{IVec3, Vec2, Vec3};

use crate::{blocks::block_def, world::World};

const HALF_WIDTH: f32 = 0.30;
const HEIGHT: f32 = 1.80;
const EYE_HEIGHT: f32 = 1.62;
const SKIN: f32 = 0.001;
const REACH: f32 = 6.0;

#[derive(Default)]
pub struct Movement {
    pub forward: f32,
    pub right: f32,
    pub sprint: bool,
    pub jump: bool,
}

pub struct Player {
    feet: Vec3,
    velocity: Vec3,
    yaw: f32,
    pitch: f32,
    grounded: bool,
    speed_boost: f32,
    jump_boost: f32,
}

#[derive(Debug, PartialEq)]
pub struct Hit {
    pub block: IVec3,
    pub adjacent: Option<IVec3>,
}

impl Player {
    pub fn new(feet: Vec3) -> Self {
        Self {
            feet,
            velocity: Vec3::ZERO,
            yaw: 0.41,
            pitch: 0.10,
            grounded: false,
            speed_boost: 0.0,
            jump_boost: 0.0,
        }
    }

    pub fn feet(&self) -> Vec3 {
        self.feet
    }

    pub fn pose(&self) -> ([f32; 3], f32, f32) {
        (self.feet.to_array(), self.yaw, self.pitch)
    }

    pub fn restore(world: &World, feet: [f32; 3], yaw: f32, pitch: f32) -> Option<Self> {
        let feet = Vec3::from_array(feet);
        if !feet.is_finite() || !yaw.is_finite() || !pitch.is_finite() {
            return None;
        }
        let (min, max) = bounds(feet);
        let first = (min + Vec3::splat(SKIN)).floor().as_ivec3();
        let last = (max - Vec3::splat(SKIN)).floor().as_ivec3();
        for z in first.z..=last.z {
            for y in first.y..=last.y {
                for x in first.x..=last.x {
                    if solid(world, IVec3::new(x, y, z)) {
                        return None;
                    }
                }
            }
        }
        let mut player = Self::new(feet);
        player.yaw = yaw.rem_euclid(TAU);
        player.pitch = pitch.clamp(-FRAC_PI_2 + 0.015, FRAC_PI_2 - 0.015);
        Some(player)
    }

    pub fn boost_speed(&mut self, seconds: f32) {
        self.speed_boost = seconds;
    }

    pub fn boost_jump(&mut self, seconds: f32) {
        self.jump_boost = seconds;
    }

    pub fn eye(&self) -> Vec3 {
        self.feet + Vec3::Y * EYE_HEIGHT
    }

    pub fn direction(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn look(&mut self, dx: f64, dy: f64) {
        const SENSITIVITY: f32 = 0.0022;
        self.yaw = (self.yaw + dx as f32 * SENSITIVITY).rem_euclid(TAU);
        self.pitch =
            (self.pitch - dy as f32 * SENSITIVITY).clamp(-FRAC_PI_2 + 0.015, FRAC_PI_2 - 0.015);
    }

    pub fn tick(&mut self, world: &World, movement: &Movement, elapsed: f32) {
        self.speed_boost = (self.speed_boost - elapsed).max(0.0);
        self.jump_boost = (self.jump_boost - elapsed).max(0.0);
        let forward = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let input = Vec2::new(movement.right, movement.forward).clamp_length_max(1.0);
        let speed = if movement.sprint { 7.6 } else { 4.8 }
            * if self.speed_boost > 0.0 { 1.5 } else { 1.0 };
        let intended = (forward * input.y + right * input.x) * speed;

        if self.grounded && movement.jump {
            self.velocity.y = if self.jump_boost > 0.0 { 11.5 } else { 8.5 };
            self.grounded = false;
        }

        // Short steps keep motion responsive after a slow frame or window resize.
        let elapsed = elapsed.min(0.08);
        let steps = (elapsed * 120.0).ceil().max(1.0) as u32;
        let dt = elapsed / steps as f32;
        for _ in 0..steps {
            let acceleration = if self.grounded { 34.0 } else { 12.0 };
            let change = Vec3::new(
                intended.x - self.velocity.x,
                0.0,
                intended.z - self.velocity.z,
            );
            self.velocity += change.clamp_length_max(acceleration * dt);
            self.velocity.y = (self.velocity.y - 25.0 * dt).max(-34.0);

            self.grounded = false;
            for axis in [0, 2, 1] {
                let delta = self.velocity[axis] * dt;
                let allowed = sweep(world, self.feet, axis, delta);
                self.feet[axis] += allowed;
                if delta != allowed {
                    self.grounded |= axis == 1 && delta < 0.0;
                    self.velocity[axis] = 0.0;
                }
            }
        }
    }

    pub fn overlaps(&self, block: IVec3) -> bool {
        let (min, max) = bounds(self.feet);
        let origin = block.as_vec3();
        min.cmplt(origin + Vec3::ONE - Vec3::splat(SKIN)).all()
            && max.cmpgt(origin + Vec3::splat(SKIN)).all()
    }

    pub fn target(&self, world: &World) -> Option<Hit> {
        trace(self.eye(), self.direction(), REACH, |cell| {
            world.block(cell) != crate::blocks::AIR
        })
    }
}

fn bounds(feet: Vec3) -> (Vec3, Vec3) {
    (
        feet + Vec3::new(-HALF_WIDTH, 0.0, -HALF_WIDTH),
        feet + Vec3::new(HALF_WIDTH, HEIGHT, HALF_WIDTH),
    )
}

fn solid(world: &World, cell: IVec3) -> bool {
    block_def(world.block(cell)).solid
}

fn sweep(world: &World, feet: Vec3, axis: usize, delta: f32) -> f32 {
    if delta == 0.0 {
        return 0.0;
    }

    let (min, max) = bounds(feet);
    let mut first = (min + Vec3::splat(SKIN)).floor().as_ivec3();
    let mut last = (max - Vec3::splat(SKIN)).floor().as_ivec3();
    first[axis] = (min[axis] + delta.min(0.0) - SKIN).floor() as i32;
    last[axis] = (max[axis] + delta.max(0.0) + SKIN).floor() as i32;

    let mut allowed = delta;
    for z in first.z..=last.z {
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                let cell = IVec3::new(x, y, z);
                if !solid(world, cell) {
                    continue;
                }

                let near = cell[axis] as f32;
                if delta > 0.0 && near >= max[axis] - SKIN {
                    allowed = allowed.min((near - max[axis] - SKIN).max(0.0));
                } else if delta < 0.0 && near + 1.0 <= min[axis] + SKIN {
                    allowed = allowed.max((near + 1.0 - min[axis] + SKIN).min(0.0));
                }
            }
        }
    }
    allowed
}

fn trace(
    origin: Vec3,
    direction: Vec3,
    limit: f32,
    is_solid: impl Fn(IVec3) -> bool,
) -> Option<Hit> {
    let mut cell = origin.floor().as_ivec3();
    let mut step = IVec3::ZERO;
    let mut crossing = Vec3::splat(f32::INFINITY);
    let mut interval = Vec3::splat(f32::INFINITY);

    for axis in 0..3 {
        let component = direction[axis];
        if component != 0.0 {
            step[axis] = if component > 0.0 { 1 } else { -1 };
            let boundary = cell[axis] + i32::from(component > 0.0);
            crossing[axis] = (boundary as f32 - origin[axis]) / component;
            interval[axis] = component.abs().recip();
        }
    }

    let mut adjacent = None;
    loop {
        if is_solid(cell) {
            return Some(Hit {
                block: cell,
                adjacent,
            });
        }

        let axis = if crossing.x <= crossing.y && crossing.x <= crossing.z {
            0
        } else if crossing.y <= crossing.z {
            1
        } else {
            2
        };
        if crossing[axis] > limit {
            return None;
        }
        adjacent = Some(cell);
        cell[axis] += step[axis];
        crossing[axis] += interval[axis];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_reports_nearest_face_across_negative_coordinates() {
        let hit = trace(Vec3::new(0.5, 2.5, 0.5), Vec3::NEG_X, 6.0, |cell| {
            cell.x == -3
        });
        assert_eq!(
            hit,
            Some(Hit {
                block: IVec3::new(-3, 2, 0),
                adjacent: Some(IVec3::new(-2, 2, 0)),
            })
        );
        assert!(trace(Vec3::ZERO, Vec3::NEG_Z, 2.0, |cell| cell.z == -4).is_none());
    }

    #[test]
    fn gravity_lands_and_jump_leaves_ground() {
        let world = World::new(184203);
        let mut player = Player::new(world.spawn());
        for _ in 0..120 {
            player.tick(&world, &Movement::default(), 1.0 / 120.0);
        }
        assert!(player.grounded);
        let standing = player.feet().y;
        player.tick(
            &world,
            &Movement {
                jump: true,
                ..Default::default()
            },
            1.0 / 60.0,
        );
        assert!(!player.grounded);
        assert!(player.feet().y > standing);
    }

    #[test]
    fn sprint_stops_at_a_wall_and_placement_checks_the_players_body() {
        let mut world = World::new(184203);
        for x in -1..=3 {
            world.set_block(IVec3::new(x, 74, 0), crate::blocks::STONE);
        }
        for y in 75..=78 {
            world.set_block(IVec3::new(2, y, 0), crate::blocks::STONE);
        }
        let mut player = Player::new(Vec3::new(0.5, 75.01, 0.5));
        player.yaw = 0.0;
        let movement = Movement {
            right: 1.0,
            sprint: true,
            ..Default::default()
        };
        for _ in 0..30 {
            player.tick(&world, &movement, 0.06);
        }
        assert!((1.69..1.70).contains(&player.feet().x));
        assert!(player.grounded);
        assert!(player.overlaps(IVec3::new(1, 75, 0)));
        assert!(!player.overlaps(IVec3::new(2, 75, 0)));
    }
}
