// Tests are allowed to assert; the library itself is not.
#![allow(clippy::disallowed_macros, clippy::unwrap_used)]

use std::cell::Cell;
use std::rc::Rc;

use weft::*;

#[derive(Debug, PartialEq, Clone, Copy)]
struct Pos(i32);
impl<S: Schema> Component<S> for Pos {}

#[derive(Debug, PartialEq, Clone, Copy)]
struct Vel(i32);
impl<S: Schema> Component<S> for Vel {}

#[derive(Debug, PartialEq)]
struct Name(&'static str);
impl<S: Schema> Component<S> for Name {}

struct Frozen;
impl<S: Schema> Component<S> for Frozen {}

/// Immutable: not `mut` in the schema.
#[derive(Debug, PartialEq)]
struct Id(u32);
impl<S: Schema> Component<S> for Id {}

#[derive(Debug, PartialEq)]
struct Likes(u32);
impl<S: Schema> Relation<S> for Likes {
    type Arity = Many;
}

struct FriendsWith;
impl<S: Schema> Relation<S> for FriendsWith {
    type Arity = Many;
    const SYMMETRIC: bool = true;
}

struct MarriedTo;
impl<S: Schema> Relation<S> for MarriedTo {
    type Arity = One;
    const SYMMETRIC: bool = true;
}

struct OwnedBy;
impl<S: Schema> Relation<S> for OwnedBy {
    type Arity = Many;
    const CASCADE: bool = true;
}

// ------------------------------------------------ hooks and required components

#[derive(Default)]
struct Log(Vec<String>);

struct Body;
impl<S: Reads<Pos> + Reads<Vel>> Component<S> for Body {
    fn requires(requires: &mut Requires<'_, S>) {
        requires.add(Pos(0)).add(Vel(0));
    }
}

struct Player;
impl<S> Component<S> for Player
where
    S: Reads<Body> + Reads<Pos> + Reads<Vel> + Reads<Name> + Owns<Log>,
{
    fn requires(requires: &mut Requires<'_, S>) {
        requires.add(Body).add(Name("anonymous"));
    }

    fn on_insert(world: &mut World<S>, entity: Entity) {
        let complete = world.has::<Vel>(entity);
        world.resource_mut::<Log>().0.push(format!("player complete: {complete}"));
    }

    fn on_remove(world: &mut World<S>, entity: Entity) {
        let name = world.get::<Name>(entity).map_or("?", |n| n.0);
        world.resource_mut::<Log>().0.push(format!("{name} leaves"));
    }
}

struct Yin;
impl<S: Reads<Yin> + Reads<Yang>> Component<S> for Yin {
    fn requires(requires: &mut Requires<'_, S>) {
        requires.add(Yang);
    }
}

struct Yang;
impl<S: Reads<Yin> + Reads<Yang>> Component<S> for Yang {
    fn requires(requires: &mut Requires<'_, S>) {
        requires.add(Yin);
    }
}

/// Counts followers on the target's `Vel`, and logs unfollows.
struct Follows;
impl<S: Writes<Vel> + Reads<Name> + Owns<Log>> Relation<S> for Follows {
    type Arity = Many;

    fn on_relate(world: &mut World<S>, _: Entity, target: Entity) {
        if let Some(mut vel) = world.get_mut::<Vel>(target) {
            vel.0 += 1;
        }
    }

    fn on_unrelate(world: &mut World<S>, source: Entity, target: Entity) {
        if let Some(mut vel) = world.get_mut::<Vel>(target) {
            vel.0 -= 1;
        }
        if let Some(name) = world.get::<Name>(source).map(|n| n.0) {
            world.resource_mut::<Log>().0.push(format!("{name} unfollows"));
        }
    }
}

schema! {
    struct Game {
        components: [mut Pos, mut Vel, Name, Frozen, Id, Body, Player, Yin, Yang],
        relations: [ChildOf, Likes, FriendsWith, MarriedTo, OwnedBy, Follows],
        resources: [Log],
    }
}

type W = World<Game>;

fn ids<F: Filter<Game>>(query: Query<'_, Game, Entity, F>) -> Vec<Entity> {
    query.iter().collect()
}

fn log(world: &W) -> Vec<String> {
    world.resource::<Log>().0.clone()
}

// ------------------------------------------------------------------ entities

#[test]
fn spawn_get_remove() {
    let mut world = W::new();
    let e = world.spawn((Pos(1), Vel(2)));
    assert_eq!(world.get::<Pos>(e), Some(&Pos(1)));
    assert_eq!(world.remove::<Vel>(e), Some(Vel(2)));
    assert!(!world.has::<Vel>(e));
    assert_eq!(world.remove::<Vel>(e), None);
    world.insert(e, Pos(5)).unwrap();
    assert_eq!(world.get::<Pos>(e), Some(&Pos(5)));
}

#[test]
fn stale_handles_stay_dead() {
    let mut world = W::new();
    let a = world.spawn(Pos(1));
    assert!(world.despawn(a));
    assert!(!world.despawn(a));
    let b = world.spawn(Pos(2));
    assert_eq!(a.index(), b.index());
    assert!(!world.is_alive(a));
    assert!(world.get::<Pos>(a).is_none());
    assert_eq!(world.insert(a, Pos(0)), Err(NoSuchEntity(a)));
    assert_eq!(world.entity_count(), 1);
}

#[test]
fn fallibility_is_in_the_type() {
    let mut world = W::new();
    // Components only: a plain `Entity`, nothing to unwrap.
    let parent: Entity = world.spawn(Name("parent"));
    // With a relation: a `Result`, because the target might be dead.
    let child: Result<Entity, NoSuchEntity> = world.spawn((Name("child"), To(ChildOf, parent)));
    assert!(child.is_ok());

    world.despawn(parent);
    let orphan = world.spawn((Name("orphan"), To(ChildOf, parent)));
    assert_eq!(orphan, Err(NoSuchEntity(parent)));
    assert_eq!(world.entity_count(), 0, "a failed spawn creates nothing");
}

#[test]
fn resources_always_exist() {
    let mut world = W::new();
    world.resource_mut::<Log>().0.push("hi".into());
    assert_eq!(log(&world), ["hi"]);
}

// ------------------------------------------------------------------- queries

#[test]
fn query_mut_mutates() {
    let mut world = W::new();
    let a = world.spawn((Pos(0), Vel(1)));
    let still = world.spawn(Pos(100));
    world.query_mut::<(&mut Pos, &Vel)>().for_each(|(mut pos, vel)| pos.0 += vel.0);
    assert_eq!(world.get::<Pos>(a), Some(&Pos(1)));
    assert_eq!(world.get::<Pos>(still), Some(&Pos(100)));
}

#[test]
fn shared_terms_can_repeat() {
    let mut world = W::new();
    world.spawn((Pos(3), Vel(4)));
    let mut seen = Vec::new();
    world.query_mut::<(&Pos, &Pos, &mut Vel)>().for_each(|(a, b, mut vel)| {
        seen.push(a.0 + b.0);
        vel.0 = 0;
    });
    assert_eq!(seen, [6]);
}

#[test]
fn read_queries() {
    let mut world = W::new();
    let a = world.spawn((Pos(0), Vel(1)));
    let b = world.spawn(Pos(1));
    let c = world.spawn(Vel(3));
    let rows: Vec<_> = world.query::<(Entity, Option<&Vel>)>().iter().map(|(e, v)| (e, v.map(|v| v.0))).collect();
    assert_eq!(rows, [(a, Some(1)), (b, None), (c, Some(3))]);
    assert!(world.query::<&Frozen>().is_empty());
    assert!(world.query::<&Pos>().get(c).is_none());
    assert!(world.query::<&Pos>().single().is_none());
    world.despawn(a);
    world.despawn(c);
    assert_eq!(world.query::<&Pos>().single(), Some(&Pos(1)));
}

#[test]
fn nested_reads_through_shared_world() {
    let mut world = W::new();
    let target = world.spawn(Pos(100));
    world.spawn((Pos(0), Vel(1)));
    // Queries over `&World` can nest freely.
    let total: Vec<i32> = world
        .query::<&Vel>()
        .iter()
        .map(|vel| vel.0 + world.query::<&Pos>().get(target).map_or(0, |p| p.0))
        .collect();
    assert_eq!(total, [101]);
}

#[test]
fn typed_filters() {
    let mut world = W::new();
    let a = world.spawn((Pos(0), Vel(0)));
    let b = world.spawn(Pos(0));
    let c = world.spawn((Pos(0), Frozen));
    let d = world.spawn(Vel(0));

    assert_eq!(ids(world.query::<Entity>().filtered::<(With<Pos>, Without<Frozen>)>()), [a, b]);
    assert_eq!(ids(world.query::<Entity>().filtered::<Or<(With<Vel>, With<Frozen>)>>()), [a, c, d]);
    assert_eq!(ids(world.query::<Entity>().filtered::<Not<Or<(With<Vel>, With<Frozen>)>>>()), [b]);

    world.clear_trackers();
    if let Some(mut pos) = world.get_mut::<Pos>(b) {
        pos.0 = 1;
    }
    let _read_only = world.get_mut::<Pos>(a).map(|p| p.0);
    world.insert(d, Pos(0)).unwrap();
    assert_eq!(ids(world.query::<Entity>().filtered::<Changed<Pos>>()), [b, d]);
    assert_eq!(ids(world.query::<Entity>().filtered::<Added<Pos>>()), [d]);
}

#[test]
fn query_mut_filters() {
    let mut world = W::new();
    let moving = world.spawn((Pos(0), Vel(1)));
    let frozen = world.spawn((Pos(0), Vel(1), Frozen));
    world
        .query_mut::<(&mut Pos, &Vel)>()
        .filtered::<Without<Frozen>>()
        .for_each(|(mut pos, vel)| pos.0 += vel.0);
    assert_eq!(world.get::<Pos>(moving), Some(&Pos(1)));
    assert_eq!(world.get::<Pos>(frozen), Some(&Pos(0)));
}

#[test]
fn view_reads_everything_the_query_does_not_borrow_mutably() {
    let mut world = W::new();
    let parent = world.spawn((Pos(10), Vel(5), Name("parent")));
    let child = world.spawn((Pos(1), To(ChildOf, parent))).unwrap();
    world
        .query_mut::<(&mut Pos, Targets<ChildOf>)>()
        .for_each_with(|(mut pos, parents), view| {
            for &parent in parents {
                pos.0 += view.get::<Vel>(parent).map_or(0, |v| v.0);
                let name = view.get::<Name>(parent).map_or("?", |n| n.0);
                view.resource_mut::<Log>().0.push(format!("child of {name}"));
            }
        });
    assert_eq!(world.get::<Pos>(child), Some(&Pos(6)));
    assert_eq!(log(&world), ["child of parent"]);
}

#[test]
fn view_defers_structural_changes() {
    let mut world = W::new();
    world.spawn((Pos(0), Vel(-1)));
    world.spawn((Pos(0), Vel(1)));
    world.query_mut::<(Entity, &Vel)>().for_each_with(|(entity, vel), view| {
        if vel.0 < 0 {
            view.defer(move |world| {
                world.despawn(entity);
            });
        }
    });
    assert_eq!(world.entity_count(), 2, "not applied yet");
    world.flush();
    assert_eq!(world.entity_count(), 1);
}

// ---------------------------------------------------------------- relations

#[test]
fn many_to_many_with_values() {
    let mut world = W::new();
    let [alice, bob, carol] = [(); 3].map(|_| world.spawn(()));
    world.relate(alice, Likes(1), bob).unwrap();
    world.relate(alice, Likes(2), carol).unwrap();
    world.relate(bob, Likes(3), carol).unwrap();

    assert_eq!(world.targets::<Likes>(alice), [bob, carol]);
    assert_eq!(world.sources::<Likes>(carol), [alice, bob]);
    assert!(!world.is_related::<Likes>(carol, bob), "directed");

    world.relate(alice, Likes(9), bob).unwrap();
    if let Some(likes) = world.relation_mut::<Likes>(alice, bob) {
        likes.0 += 1;
    }
    assert_eq!(world.relation::<Likes>(alice, bob), Some(&Likes(10)));
    assert_eq!(world.targets::<Likes>(alice), [bob, carol], "replacing keeps order");

    assert_eq!(world.unrelate::<Likes>(alice, carol), Some(Likes(2)));
    assert_eq!(world.sources::<Likes>(carol), [bob]);

    world.despawn(bob);
    assert!(world.targets::<Likes>(alice).is_empty());
    assert!(world.sources::<Likes>(carol).is_empty());
}

#[test]
fn symmetric_and_exclusive() {
    let mut world = W::new();
    let [a, b, c] = [(); 3].map(|_| world.spawn(()));
    world.relate(a, FriendsWith, b).unwrap();
    world.relate(c, FriendsWith, a).unwrap();
    assert!(world.is_related::<FriendsWith>(b, a));
    assert_eq!(world.targets::<FriendsWith>(a), [b, c]);
    world.unrelate::<FriendsWith>(b, a);
    assert_eq!(world.targets::<FriendsWith>(a), [c]);

    world.relate(a, MarriedTo, b).unwrap();
    world.relate(c, MarriedTo, b).unwrap();
    assert_eq!(world.target::<MarriedTo>(b), Some(c));
    assert_eq!(world.target::<MarriedTo>(a), None, "a was divorced when b remarried");
}

#[test]
fn hierarchy() {
    let mut world = W::new();
    let root = world.spawn(Name("root"));
    let a = world.spawn((Name("a"), To(ChildOf, root))).unwrap();
    let b = world.spawn((Name("b"), To(ChildOf, root))).unwrap();
    let a1 = world.spawn((Name("a1"), To(ChildOf, a))).unwrap();

    assert_eq!(world.target::<ChildOf>(a1), Some(a));
    assert_eq!(world.ancestors::<ChildOf>(a1).collect::<Vec<_>>(), [a, root]);
    assert_eq!(world.descendants::<ChildOf>(root).collect::<Vec<_>>(), [a, b, a1]);
    assert_eq!(ids(world.query::<Entity>().filtered::<Not<HasTarget<ChildOf>>>()), [root]);
    assert_eq!(ids(world.query::<Entity>().filtered::<Not<HasSource<ChildOf>>>()), [b, a1]);

    world.relate(a1, ChildOf, b).unwrap();
    assert!(world.sources::<ChildOf>(a).is_empty(), "reparenting is exclusive");

    world.despawn(root);
    assert_eq!(world.entity_count(), 0, "cascade");
}

#[test]
fn cascade_is_cycle_safe_and_selective() {
    let mut world = W::new();
    let [a, b, c, bystander] = [(); 4].map(|_| world.spawn(()));
    world.relate(a, OwnedBy, b).unwrap();
    world.relate(b, OwnedBy, c).unwrap();
    world.relate(c, OwnedBy, a).unwrap();
    world.relate(bystander, Likes(0), a).unwrap();
    world.despawn(a);
    assert!(!world.is_alive(b) && !world.is_alive(c));
    assert!(world.is_alive(bystander));
    assert!(world.targets::<Likes>(bystander).is_empty());
}

#[test]
fn traversal_handles_cycles() {
    let mut world = W::new();
    let [a, b, c] = [(); 3].map(|_| world.spawn(()));
    world.relate(a, Likes(0), b).unwrap();
    world.relate(b, Likes(0), c).unwrap();
    world.relate(c, Likes(0), a).unwrap();
    assert_eq!(world.ancestors::<Likes>(a).collect::<Vec<_>>(), [b, c]);
    assert_eq!(world.descendants::<Likes>(a).collect::<Vec<_>>(), [c, b]);
}

#[test]
fn relation_terms_in_read_queries() {
    let mut world = W::new();
    let parent = world.spawn(Pos(10));
    world.spawn((Pos(1), To(ChildOf, parent))).unwrap();
    let child_counts: Vec<_> = world.query::<Sources<ChildOf>>().iter().map(<[_]>::len).collect();
    assert_eq!(child_counts, [1, 0]);
}

// -------------------------------------------------------------------- hooks

#[test]
fn required_components() {
    let mut world = W::new();
    let e = world.spawn((Player, Name("zoe"), Pos(5)));
    assert!(world.has::<Body>(e) && world.has::<Vel>(e), "transitive");
    assert_eq!(world.get::<Name>(e), Some(&Name("zoe")), "explicit value wins");
    assert_eq!(world.get::<Pos>(e), Some(&Pos(5)), "explicit value wins");
    let bare = world.spawn(Player);
    assert_eq!(world.get::<Name>(bare), Some(&Name("anonymous")));
    let yin = world.spawn(Yin);
    assert!(world.has::<Yang>(yin), "cyclic requirements terminate");
}

#[test]
fn component_hooks_see_complete_entities() {
    let mut world = W::new();
    let e = world.spawn((Player, Name("zoe")));
    world.despawn(e);
    assert_eq!(log(&world), ["player complete: true", "zoe leaves"]);
}

#[test]
fn relation_hooks() {
    let mut world = W::new();
    let star = world.spawn(Vel(0));
    let owner = world.spawn(());
    let fan = world.spawn((Name("fan"), To(Follows, star), To(OwnedBy, owner))).unwrap();
    world.spawn(To(Follows, star)).unwrap();
    assert_eq!(world.get::<Vel>(star), Some(&Vel(2)));
    world.despawn(owner); // cascades to fan
    assert!(!world.is_alive(fan));
    assert_eq!(world.get::<Vel>(star), Some(&Vel(1)));
    assert_eq!(log(&world), ["fan unfollows"], "source still intact during cascade");
}

#[test]
fn immutable_components_can_be_read_and_replaced() {
    let mut world = W::new();
    let e = world.spawn(Id(1));
    assert_eq!(world.query::<&Id>().single(), Some(&Id(1)));
    world.insert(e, Id(2)).unwrap();
    assert_eq!(world.get::<Id>(e), Some(&Id(2)));
}

// ------------------------------------------------------------------- events

struct Click;
impl<S: Links<ChildOf>> EntityEvent<S> for Click {
    fn propagate(world: &World<S>, entity: Entity) -> Option<Entity> {
        world.target::<ChildOf>(entity)
    }
}

struct Tick;
impl Event for Tick {}

#[test]
fn entity_events_bubble_until_stopped() {
    let mut world = W::new();
    let window = world.spawn(Name("window"));
    let panel = world.spawn((Name("panel"), To(ChildOf, window))).unwrap();
    let button = world.spawn((Name("button"), To(ChildOf, panel))).unwrap();
    for e in [window, panel, button] {
        world
            .observe_entity(e, |world, on: On<Click>| {
                let name = world.get::<Name>(on.entity()).map_or("?", |n| n.0);
                world.resource_mut::<Log>().0.push(name.into());
                if name == "panel" {
                    on.stop_propagation();
                }
            })
            .unwrap();
    }
    world.observe_entities(|world, _: On<Click>| world.resource_mut::<Log>().0.push("any".into()));

    world.trigger_at(button, Click).unwrap();
    assert_eq!(log(&world), ["button", "any", "panel", "any"]);
}

#[test]
fn global_events_and_unobserve() {
    let hits = Rc::new(Cell::new(0));
    let mut world = W::new();
    let h = hits.clone();
    let id = world.observe(move |_, _: &Tick| h.set(h.get() + 1));
    world.trigger(Tick);
    assert!(world.unobserve(id));
    assert!(!world.unobserve(id));
    world.trigger(Tick);
    assert_eq!(hits.get(), 1);
}

#[test]
fn entity_observers_die_with_their_entity() {
    let hits = Rc::new(Cell::new(0));
    let mut world = W::new();
    let a = world.spawn(());
    let h = hits.clone();
    world.observe_entity(a, move |_, _: On<Click>| h.set(h.get() + 1)).unwrap();
    world.trigger_at(a, Click).unwrap();
    world.despawn(a);
    assert_eq!(world.trigger_at(a, Click), Err(NoSuchEntity(a)));
    let reused = world.spawn(());
    assert_eq!(reused.index(), a.index());
    world.trigger_at(reused, Click).unwrap();
    assert_eq!(hits.get(), 1);
}

// ---------------------------------------------------------------- schedules

#[test]
fn each_system_sees_changes_since_its_own_last_run() {
    let mut world = W::new();
    world.spawn(Pos(0));
    let mut schedule = Schedule::new();
    let mut runs = 0;
    schedule
        .add(move |world: &mut W| {
            runs += 1;
            if runs <= 2 {
                world.query_mut::<&mut Pos>().for_each(|mut pos| pos.0 += 1);
            }
        })
        .add(|world: &mut W| {
            let changed: Vec<i32> = world.query::<&Pos>().filtered::<Changed<Pos>>().iter().map(|p| p.0).collect();
            for value in changed {
                world.resource_mut::<Log>().0.push(format!("changed to {value}"));
            }
        })
        .add(|world: &mut W| {
            world.defer(|world| {
                world.spawn(Name("deferred"));
            });
        })
        .add(|world: &mut W| {
            let count = world.query::<&Name>().iter().count();
            world.resource_mut::<Log>().0.push(format!("{count} names"));
        });
    for _ in 0..3 {
        schedule.run(&mut world);
    }
    assert_eq!(log(&world), ["changed to 1", "1 names", "changed to 2", "2 names", "3 names"]);
}

// ------------------------------------------------ capability-bounded plugins

/// Works with any world that has `Pos` (writable) and `Vel`.
fn physics<S: Borrows<(&'static mut Pos, &'static Vel)>>(world: &mut World<S>) {
    world.query_mut::<(&mut Pos, &Vel)>().for_each(|(mut pos, vel)| pos.0 += vel.0);
}

schema! {
    struct Tiny {
        components: [mut Pos, Vel],
        relations: [],
        resources: [],
    }
}

#[test]
fn generic_systems_run_on_any_capable_schema() {
    let mut game = W::new();
    let mut tiny = World::<Tiny>::new();
    let a = game.spawn((Pos(0), Vel(2)));
    let b = tiny.spawn((Pos(0), Vel(3)));
    physics(&mut game);
    physics(&mut tiny);
    assert_eq!(game.get::<Pos>(a), Some(&Pos(2)));
    assert_eq!(tiny.get::<Pos>(b), Some(&Pos(3)));
}
