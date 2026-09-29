//! Many-to-many relations with values, a cascading relation, hooks, a view,
//! and a schedule.

use weft::*;

struct Name(&'static str);
impl<S: Schema> Component<S> for Name {}

struct Student;
impl<S: Schema> Component<S> for Student {}

/// `student --Enrolled--> class`, valued with the student's grade.
struct Enrolled {
    grade: u32,
}
impl<S: Reads<Name>> Relation<S> for Enrolled {
    type Arity = Many;

    fn on_unrelate(world: &mut World<S>, student: Entity, class: Entity) {
        println!("  {} dropped out of {}", name(world, student), name(world, class));
    }
}

/// `class --TaughtBy--> teacher`. When a teacher leaves, their classes go too.
struct TaughtBy;
impl<S: Schema> Relation<S> for TaughtBy {
    type Arity = One;
    const CASCADE: bool = true;
}

#[derive(Default)]
struct Term(u32);

schema! {
    struct School {
        components: [Name, Student],
        relations: [Enrolled, TaughtBy],
        resources: [Term],
    }
}

fn name<S: Reads<Name>>(world: &World<S>, e: Entity) -> &'static str {
    world.get::<Name>(e).map_or("?", |n| n.0)
}

fn report(world: &mut World<School>) {
    println!("term {}:", world.resource::<Term>().0);
    for (student, classes) in world.query::<(Entity, Targets<Enrolled>)>().filtered::<With<Student>>() {
        let grades: Vec<String> = classes
            .iter()
            .map(|&class| {
                let grade = world.relation::<Enrolled>(student, class).map_or(0, |e| e.grade);
                format!("{} ({grade})", name(world, class))
            })
            .collect();
        println!("  {} takes: {}", name(world, student), grades.join(", "));
    }
}

fn main() -> Result<(), NoSuchEntity> {
    let mut world = World::<School>::new();
    let snape = world.spawn(Name("Snape"));
    let mcgonagall = world.spawn(Name("McGonagall"));
    let potions = world.spawn((Name("Potions"), To(TaughtBy, snape)))?;
    let transfiguration = world.spawn((Name("Transfiguration"), To(TaughtBy, mcgonagall)))?;

    for (student, [p, t]) in [("Harry", [60, 80]), ("Hermione", [99, 100]), ("Ron", [55, 70])] {
        world.spawn((
            Name(student),
            Student,
            To(Enrolled { grade: p }, potions),
            To(Enrolled { grade: t }, transfiguration),
        ))?;
    }

    let mut schedule = Schedule::new();
    schedule
        .add(move |world: &mut World<School>| {
            // Snape curves his classes by 10.
            let curved: Vec<(Entity, Entity)> = world
                .sources::<TaughtBy>(snape)
                .iter()
                .flat_map(|&class| world.sources::<Enrolled>(class).iter().map(move |&student| (student, class)))
                .collect();
            for (student, class) in curved {
                if let Some(enrolled) = world.relation_mut::<Enrolled>(student, class) {
                    enrolled.grade += 10;
                }
            }
            world.resource_mut::<Term>().0 += 1;
        })
        .add(report);
    schedule.run(&mut world);

    println!("\nSnape leaves:");
    world.despawn(snape);
    report(&mut world);
    Ok(())
}
