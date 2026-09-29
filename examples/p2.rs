#![feature(decl_macro)]
use weft::*;

#[derive(Debug)]
pub struct Settings;
kind!(root Settings);

#[derive(Debug)]
pub struct Npc {
    pub name: &'static str,
}
kind!(Npc);

// How well two NPCs know each other, from each side. One fact, two values.
relation!(pub struct Knows(pub u8): Npc <-> Npc, many, cyclic, detach);
// How much the source likes the target. Not mutual.
relation!(pub struct Likes(pub u8): Npc -> Npc, many, cyclic, detach);

module!(pub Village { kinds: [Npc], relations: [Knows, Likes] });
game!(pub Game: Settings [Village]);

/// Everyone `who` likes, most-liked first.
fn liked_by<'t, G: Village>(t: &Turn<'t, G>, who: Id<'t, Npc>) -> Vec<(&'static str, u8)> {
    let mut liked: Vec<_> = t.targets::<Likes>(who).map(|(npc, likes)| (t[npc].name, likes.0)).collect();
    liked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    liked
}

fn main() {
    let mut world = World::<Game>::new(|t| {
        let [smith, baker, miller, witch] =
            ["smith", "baker", "miller", "witch"].map(|name| t.spawn(Npc { name }, ()));
        t.relate(smith, (Knows(9), Knows(7)), baker);
        t.relate(smith, (Knows(3), Knows(8)), witch);
        t.relate(smith, Likes(8), baker);
        t.relate(smith, Likes(2), miller);
        t.relate(smith, Likes(5), witch);
        t.relate(baker, Likes(9), smith);
        t.spawn(Settings, ())
    });

    world.turn(|t| {
        let Some(smith) = t.all::<Npc>().find(|&npc| t[npc].name == "smith") else { return };

        let liked = liked_by(t, smith);
        assert_eq!(liked, [("baker", 8), ("witch", 5), ("miller", 2)]);
        for (name, how_much) in liked {
            println!("{name}: {how_much}");
        }

        // A join over two relations: people the smith likes *and* knows, with how well.
        let both: Vec<_> = t
            .targets::<Likes>(smith)
            .filter_map(|(npc, likes)| {
                let knows = t.targets::<Knows>(smith).find(|&(k, _)| k == npc)?;
                Some((t[npc].name, likes.0, knows.1 .0))
            })
            .collect();
        assert_eq!(both, [("baker", 8, 9), ("witch", 5, 3)]);
        for (name, likes, knows) in both {
            println!("{name} (likes {likes}, knows {knows})");
        }
    });
}
