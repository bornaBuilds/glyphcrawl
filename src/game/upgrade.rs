//! Upgrades: data-driven modifiers applied on top of a class's base stats and abilities.

use super::actor::{Actor, Stats};
use super::defs::{AbilityDef, AbilityField, AreaDef, ClassDef, Defs, Effect, Modifier, StatKind, UpgradeDef};
use super::rng::Rng;

fn apply_num(v: &mut f32, add: f32, mul: f32) {
    *v = (*v + add) * (1.0 + mul);
}

fn apply_u32(v: &mut u32, add: f32, mul: f32) {
    let mut f = *v as f32;
    apply_num(&mut f, add, mul);
    *v = f.round().max(0.0) as u32;
}

fn area_field(a: &mut AreaDef, f: AbilityField, add: f32, mul: f32) {
    match f {
        AbilityField::Damage => {
            apply_num(&mut a.dmg, 0.0, mul);
            apply_num(&mut a.linger_dps, 0.0, mul);
            a.dmg += add;
        }
        AbilityField::Radius => apply_num(&mut a.radius, add, mul),
        AbilityField::Linger => {
            apply_num(&mut a.linger, add, mul);
            if a.linger > 0.0 && a.linger_dps <= 0.0 {
                a.linger_dps = (a.dmg * 0.4).max(4.0);
            }
        }
        AbilityField::Pulses => apply_u32(&mut a.pulses, add, mul),
        AbilityField::Knockback => apply_num(&mut a.knockback, add, mul),
        AbilityField::StatusDur => {
            for s in &mut a.on_hit {
                apply_num(&mut s.dur, add, mul);
            }
        }
        _ => {}
    }
}

/// Modify one field of an ability (and every effect inside it that has that field).
pub fn modify_ability(def: &mut AbilityDef, f: AbilityField, add: f32, mul: f32) {
    match f {
        AbilityField::Cooldown => return apply_num(&mut def.cooldown, add, mul),
        AbilityField::Cost => return apply_num(&mut def.cost, add, mul),
        _ => {}
    }
    for e in &mut def.effects {
        match e {
            Effect::MeleeArc { range, arc, dmg, max_targets, knockback, on_hit, .. } => match f {
                AbilityField::Damage => apply_num(dmg, add, mul),
                AbilityField::Range => apply_num(range, add, mul),
                AbilityField::Arc => apply_num(arc, add, mul),
                AbilityField::MaxTargets => apply_u32(max_targets, add, mul),
                AbilityField::Knockback => apply_num(knockback, add, mul),
                AbilityField::StatusDur => on_hit.iter_mut().for_each(|s| apply_num(&mut s.dur, add, mul)),
                _ => {}
            },
            Effect::Projectile { range, dmg, count, spread, pierce, knockback, on_hit, explode, .. } => match f {
                AbilityField::Damage => {
                    apply_num(dmg, add, mul);
                    if let Some(a) = explode {
                        area_field(a, f, 0.0, mul);
                    }
                }
                AbilityField::Range => apply_num(range, add, mul),
                AbilityField::Count => apply_u32(count, add, mul),
                AbilityField::Spread => apply_num(spread, add, mul),
                AbilityField::Pierce => apply_u32(pierce, add, mul),
                AbilityField::Knockback => apply_num(knockback, add, mul),
                AbilityField::StatusDur => on_hit.iter_mut().for_each(|s| apply_num(&mut s.dur, add, mul)),
                AbilityField::Radius | AbilityField::Linger | AbilityField::Pulses => {
                    if let Some(a) = explode {
                        area_field(a, f, add, mul);
                    }
                }
                _ => {}
            },
            Effect::AreaAtAim { area, max_range } => {
                if f == AbilityField::Range {
                    apply_num(max_range, add, mul);
                } else {
                    area_field(area, f, add, mul);
                }
            }
            Effect::AreaSelf { area } => area_field(area, f, add, mul),
            Effect::Dash { dist, contact_dmg, knockback, on_hit, .. } => match f {
                AbilityField::Damage => apply_num(contact_dmg, add, mul),
                AbilityField::Range => apply_num(dist, add, mul),
                AbilityField::Knockback => apply_num(knockback, add, mul),
                AbilityField::StatusDur => on_hit.iter_mut().for_each(|s| apply_num(&mut s.dur, add, mul)),
                _ => {}
            },
            Effect::Blink { max_range } => {
                if f == AbilityField::Range {
                    apply_num(max_range, add, mul);
                }
            }
            Effect::Block { arc, reduction, .. } => match f {
                AbilityField::Arc => apply_num(arc, add, mul),
                AbilityField::Damage => apply_num(reduction, add, mul),
                _ => {}
            },
            Effect::Buff { statuses } => {
                if f == AbilityField::StatusDur {
                    statuses.iter_mut().for_each(|s| apply_num(&mut s.dur, add, mul));
                }
            }
            Effect::Leap { land: Some(a), .. } => area_field(a, f, add, mul),
            _ => {}
        }
    }
}

/// Rebuild a hero's stats and abilities from the class definition + every upgrade taken.
pub fn rebuild_hero(a: &mut Actor, class: &ClassDef, defs: &Defs) {
    let hp_frac = a.hp / a.stats.max_hp.max(1.0);
    let mut stats = Stats { max_hp: class.hp, armor: class.armor, speed: class.speed, dmg_mult: 1.0, crit: class.crit, cdr: 0.0 };
    let mut res_max = class.resource.max;
    let mut abilities: Vec<AbilityDef> = class.abilities.iter().map(|id| defs.abilities[id].clone()).collect();
    for (id, stacks) in &a.upgrades {
        let Some(u) = defs.upgrades.iter().find(|u| &u.id == id) else { continue };
        for _ in 0..*stacks {
            for m in &u.mods {
                match m {
                    Modifier::Stat { stat, add, mul } => {
                        let v = match stat {
                            StatKind::MaxHp => &mut stats.max_hp,
                            StatKind::Armor => &mut stats.armor,
                            StatKind::Speed => &mut stats.speed,
                            StatKind::Damage => &mut stats.dmg_mult,
                            StatKind::Crit => &mut stats.crit,
                            StatKind::Cdr => &mut stats.cdr,
                            StatKind::ResourceMax => &mut res_max,
                        };
                        apply_num(v, *add, *mul);
                    }
                    Modifier::Ability { slot, field, add, mul } => {
                        if let Some(ab) = abilities.get_mut(*slot) {
                            modify_ability(ab, *field, *add, *mul);
                        }
                    }
                    Modifier::AddOnHit { slot, status } => {
                        if let Some(ab) = abilities.get_mut(*slot) {
                            for e in &mut ab.effects {
                                match e {
                                    Effect::MeleeArc { on_hit, .. } | Effect::Projectile { on_hit, .. } | Effect::Dash { on_hit, .. } => {
                                        on_hit.push(*status)
                                    }
                                    Effect::AreaAtAim { area, .. } | Effect::AreaSelf { area } => area.on_hit.push(*status),
                                    _ => {}
                                }
                            }
                        }
                    }
                    Modifier::Heal(_) => {}
                }
            }
        }
    }
    stats.cdr = stats.cdr.clamp(0.0, 0.6);
    a.stats = stats;
    a.hp = (hp_frac * a.stats.max_hp).min(a.stats.max_hp);
    if let Some(r) = &mut a.resource {
        r.max = res_max;
        r.cur = r.cur.min(r.max);
    }
    for (slot, def) in a.slots.iter_mut().zip(abilities) {
        slot.def = def;
    }
}

/// Take an upgrade: record it, apply one-off effects (heals), rebuild.
pub fn take_upgrade(a: &mut Actor, u: &UpgradeDef, class: &ClassDef, defs: &Defs) {
    match a.upgrades.iter_mut().find(|(id, _)| *id == u.id) {
        Some(e) => e.1 += 1,
        None => a.upgrades.push((u.id.clone(), 1)),
    }
    rebuild_hero(a, class, defs);
    for m in &u.mods {
        if let Modifier::Heal(h) = m {
            a.hp = (a.hp + h).min(a.stats.max_hp);
        }
    }
}

/// Offer up to three different upgrades the hero can still take.
pub fn offer(rng: &mut Rng, defs: &Defs, a: &Actor) -> Vec<usize> {
    let class = a.class_id().unwrap_or("");
    let mut pool: Vec<usize> = defs
        .upgrades
        .iter()
        .enumerate()
        .filter(|(_, u)| u.class.as_deref().is_none_or(|c| c == class))
        .filter(|(_, u)| a.upgrades.iter().find(|(id, _)| *id == u.id).map(|e| e.1).unwrap_or(0) < u.max)
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    while out.len() < 3 && !pool.is_empty() {
        let weights: Vec<u32> = pool
            .iter()
            .map(|&i| {
                let u = &defs.upgrades[i];
                // Class upgrades are a bit more likely, so runs feel distinct.
                if u.class.is_some() { u.weight * 3 / 2 } else { u.weight }
            })
            .collect();
        let k = rng.pick_weighted(&weights).unwrap_or(0);
        out.push(pool.remove(k));
    }
    out
}
