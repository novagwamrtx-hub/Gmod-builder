//! Starting points for Discover, for people who do not know what to type.
//!
//! Each was tried against the Workshop in October 2026 and kept only if at
//! least a quarter of the best-rated matching collections were on the theme.

/// What the user sees, and what is searched for.
pub struct Idea {
    pub name: &'static str,
    pub search: &'static str,
}

const fn idea(name: &'static str, search: &'static str) -> Idea {
    Idea { name, search }
}

/// Shown before the user has typed anything.
pub const POPULAR: [&str; 8] =
    ["Zombie Survival", "TTT", "DarkRP", "Military RP", "Star Wars RP", "SCP RP", "Horror", "Prop Hunt"];

#[rustfmt::skip]
pub const IDEAS: &[(&str, &[Idea])] = &[
    ("Gamemodes", &[
        idea("Sandbox", "sandbox"),
        idea("TTT", "ttt"),
        idea("TTT2", "ttt2"),
        idea("Murder", "murder"),
        idea("Prop Hunt", "prop hunt"),
        idea("Deathrun", "deathrun"),
        idea("Jailbreak", "jailbreak"),
        idea("Zombie Survival", "zombie survival"),
        idea("Hide and Seek", "hide and seek"),
        idea("Bunny hop", "bhop"),
        idea("Surf", "surf"),
        idea("Cinema", "cinema"),
        idea("Flood", "flood"),
        idea("Stranded", "stranded"),
        idea("Homicide", "homicide"),
        idea("Melon Bomber", "melon bomber"),
        idea("Guess Who", "guess who"),
        idea("Slashers", "slashers"),
        idea("Nazi Zombies (nZombies)", "nzombies"),
        idea("Horde", "horde"),
        idea("Basewars", "basewars"),
        idea("Battle Royale", "battle royale"),
        idea("Team Deathmatch", "tdm"),
        idea("Gun Game", "gun game"),
        idea("Parkour", "parkour"),
        idea("Minigames", "minigames"),
        idea("Sledbuild", "sledbuild"),
        idea("Extreme Football Throwdown", "extreme football throwdown"),
        idea("Garry's Mod Tower", "gmod tower"),
        idea("Morbus", "morbus"),
        idea("Stop it Slender", "slender"),
        idea("The Hidden", "the hidden"),
        idea("Zombie Escape", "zombie escape"),
        idea("Infection", "infection"),
        idea("Spacebuild", "spacebuild"),
        idea("Climb", "climb"),
        idea("Death race", "race"),
    ]),
    ("Roleplay", &[
        idea("Military RP", "military rp"),
        idea("Star Wars RP", "star wars rp"),
        idea("Clone Wars RP", "clone wars rp"),
        idea("Imperial RP", "imperial rp"),
        idea("SCP RP", "scp rp"),
        idea("Half-Life 2 RP", "hl2rp"),
        idea("Police RP", "police rp"),
        idea("School RP", "school rp"),
        idea("Hogwarts RP", "hogwarts"),
        idea("Medieval RP", "medieval"),
        idea("Fallout RP", "fallout"),
        idea("Metro RP", "metro"),
        idea("STALKER RP", "stalker"),
        idea("WW2 RP", "ww2"),
        idea("1942 RP", "1942"),
        idea("Vietnam RP", "vietnam"),
        idea("Cold War RP", "cold war"),
        idea("Zombie RP", "zombie rp"),
        idea("Prison RP", "prison rp"),
        idea("Mafia RP", "mafia"),
        idea("City RP", "city rp"),
        idea("Halo RP", "halo"),
        idea("Warhammer 40k RP", "warhammer 40k"),
        idea("Naruto RP", "naruto"),
        idea("Anime RP", "anime rp"),
        idea("Purge RP", "purge"),
        idea("Hospital RP", "hospital"),
        idea("Wild West RP", "western"),
        idea("Cyberpunk RP", "cyberpunk"),
        idea("Apocalypse RP", "apocalypse"),
        idea("Backrooms RP", "backrooms"),
        idea("Jedi vs Sith", "jedi"),
    ]),
    ("Games and films", &[
        idea("DarkRP", "darkrp"),
        idea("Mass Effect", "mass effect"),
        idea("Stargate", "stargate"),
        idea("Star Trek", "star trek"),
        idea("Marvel superheroes", "marvel"),
        idea("DC superheroes", "batman"),
        idea("Attack on Titan", "attack on titan"),
        idea("Dragon Ball", "dragon ball"),
        idea("Pokemon", "pokemon"),
        idea("Minecraft", "minecraft"),
        idea("Five Nights at Freddy's", "fnaf"),
        idea("Team Fortress 2", "tf2"),
        idea("Left 4 Dead", "left 4 dead"),
        idea("Call of Duty", "call of duty"),
        idea("Resident Evil", "resident evil"),
        idea("Silent Hill", "silent hill"),
        idea("Undertale", "undertale"),
        idea("Sonic", "sonic"),
        idea("Mario", "mario"),
        idea("Portal", "portal"),
        idea("GTA", "gta"),
        idea("Payday", "payday"),
        idea("Rainbow Six Siege", "rainbow six"),
        idea("Battlefield", "battlefield"),
        idea("Titanfall", "titanfall"),
        idea("Doom", "doom"),
        idea("Dark Souls", "dark souls"),
        idea("Skyrim", "skyrim"),
        idea("Harry Potter", "harry potter"),
        idea("Jurassic Park dinosaurs", "dinosaur"),
        idea("Godzilla and kaiju", "godzilla"),
        idea("Transformers", "transformers"),
        idea("Lethal Company", "lethal company"),
        idea("Among Us", "among us"),
        idea("Squid Game", "squid game"),
        idea("Poppy Playtime", "poppy playtime"),
        idea("Bendy", "bendy"),
        idea("Skibidi", "skibidi"),
        idea("Hello Neighbor", "hello neighbor"),
        idea("Garten of Banban", "banban"),
        idea("Roblox", "roblox"),
        idea("Fortnite", "fortnite"),
        idea("Overwatch", "overwatch"),
        idea("Counter-Strike", "csgo"),
        idea("Half-Life", "half-life"),
        idea("Black Mesa", "black mesa"),
        idea("Combine", "combine"),
        idea("Hololive and VTubers", "hololive"),
        idea("My Little Pony", "mlp"),
        idea("Furry", "furry"),
    ]),
    ("Horror", &[
        idea("Horror", "horror"),
        idea("Horror maps", "horror maps"),
        idea("Nextbots", "nextbot"),
        idea("Zombies", "zombie"),
        idea("Zombie apocalypse", "zombie apocalypse"),
        idea("SCP", "scp"),
        idea("Backrooms", "backrooms"),
        idea("Creepypasta", "creepypasta"),
        idea("Slender", "slenderman"),
        idea("Liminal spaces", "liminal"),
        idea("Trevor Henderson", "trevor henderson"),
        idea("Analog horror", "analog horror"),
        idea("Dead by Daylight", "dead by daylight"),
        idea("Outlast", "outlast"),
        idea("Cry of Fear", "cry of fear"),
        idea("Amnesia", "amnesia"),
        idea("Alien", "alien"),
        idea("The Thing", "the thing"),
        idea("Siren Head", "siren head"),
    ]),
    ("Weapons and combat", &[
        idea("Realistic weapons", "realistic weapons"),
        idea("Realism", "realism"),
        idea("Modern Warfare weapons", "mw base"),
        idea("ARC9 weapons", "arc9"),
        idea("ArcCW weapons", "arccw"),
        idea("TFA weapons", "tfa"),
        idea("CW 2.0 weapons", "cw 2.0"),
        idea("M9K weapons", "m9k"),
        idea("TacRP weapons", "tacrp"),
        idea("MW2019 SWEPs", "mw2019"),
        idea("Tactical", "tactical"),
        idea("NPC battles", "npc battle"),
        idea("NPC wars", "npc wars"),
        idea("VJ Base SNPCs", "vj base"),
        idea("DrGBase nextbots", "drgbase"),
        idea("Military", "military"),
        idea("War", "war"),
        idea("WW2", "world war 2"),
        idea("WW1", "ww1"),
        idea("Modern military", "modern military"),
        idea("Special forces", "special forces"),
        idea("Tanks", "tanks"),
        idea("Helicopters and planes (LFS)", "lfs"),
        idea("LVS vehicles", "lvs"),
        idea("WAC aircraft", "wac"),
        idea("Gredwitch", "gredwitch"),
        idea("ACF armoured combat", "acf"),
        idea("Melee and swords", "melee"),
        idea("Lightsabers", "lightsaber"),
        idea("Magic and wands", "magic"),
        idea("Explosives and nukes", "nuke"),
        idea("Gore and blood", "gore"),
        idea("Ragdoll physics", "ragdoll"),
        idea("Bodycam", "bodycam"),
        idea("PvP", "pvp"),
        idea("PvE co-op", "coop"),
        idea("Campaign maps", "campaign"),
        idea("Half-Life 2 campaign", "hl2 campaign"),
        idea("Survival", "survival"),
        idea("Stealth", "stealth"),
        idea("Sci-fi weapons", "sci-fi"),
        idea("Futuristic", "futuristic"),
        idea("Mechs", "mech"),
        idea("Drones", "drones"),
        idea("Admin tools", "admin"),
    ]),
    ("Vehicles and building", &[
        idea("Building", "build"),
        idea("Wiremod", "wiremod"),
        idea("Contraptions", "contraption"),
        idea("Cars", "cars"),
        idea("Simfphys cars", "simfphys"),
        idea("TDM cars", "tdm cars"),
        idea("Photon emergency vehicles", "photon"),
        idea("Glide vehicles", "glide"),
        idea("Trains", "trains"),
        idea("Metrostroi", "metrostroi"),
        idea("Boats and ships", "ships"),
        idea("Planes", "planes"),
        idea("Spaceships", "spaceship"),
        idea("Racing", "racing"),
        idea("Drifting", "drift"),
        idea("Trucks", "trucks"),
        idea("Elevators", "elevator"),
        idea("Space and planets", "space"),
        idea("Robots", "robot"),
        idea("PAC3", "pac3"),
        idea("Tools", "tools"),
        idea("Precision building", "precision"),
        idea("Props", "props"),
        idea("Furniture props", "furniture"),
        idea("Food props", "food"),
        idea("Destruction", "destruction"),
        idea("Physics toys", "physics"),
        idea("Fireworks", "fireworks"),
        idea("Natural disasters", "disasters"),
        idea("Tornado (gDisasters)", "gdisasters"),
        idea("Titanic and sinking ships", "titanic"),
        idea("Water and oceans", "water"),
        idea("Weather", "weather"),
        idea("StarfallEx", "starfall"),
    ]),
    ("Maps, looks and extras", &[
        idea("Posing and screenshots", "posing"),
        idea("Scenebuilding", "scenebuild"),
        idea("Machinima", "machinima"),
        idea("Animation", "animation"),
        idea("Ragdolls", "ragdolls"),
        idea("Playermodels", "playermodels"),
        idea("Anime playermodels", "anime"),
        idea("Effects and particles", "effects"),
        idea("Graphics and shaders", "graphics"),
        idea("Reshade style", "shaders"),
        idea("HUD", "hud"),
        idea("Sounds", "sounds"),
        idea("Music", "music"),
        idea("Memes", "memes"),
        idea("Funny", "funny"),
        idea("Cursed", "cursed"),
        idea("Cinematic", "cinematic"),
        idea("Map packs", "maps"),
        idea("City maps", "city"),
        idea("Forest maps", "forest"),
        idea("Snow and winter", "winter"),
        idea("Desert", "desert"),
        idea("Island", "island"),
        idea("Night maps", "night"),
        idea("Urban", "urban"),
        idea("Abandoned places", "abandoned"),
        idea("Soviet and post-Soviet", "soviet"),
        idea("Japan", "japan"),
        idea("School", "school"),
        idea("Mall", "mall"),
        idea("Bunker", "bunker"),
        idea("Construct maps", "gm_construct"),
        idea("Flatgrass maps", "flatgrass"),
        idea("Christmas", "christmas"),
        idea("Halloween", "halloween"),
        idea("Retro and 90s", "retro"),
        idea("Vaporwave", "vaporwave"),
        idea("Nostalgia (old GMod)", "nostalgia"),
        idea("Quality of life", "quality of life"),
        idea("Essentials", "essentials"),
        idea("Must have", "must have"),
        idea("Vanilla plus", "vanilla"),
        idea("Optimization and FPS", "fps"),
        idea("Friends server", "friends"),
        idea("Co-op with friends", "multiplayer"),
        idea("Singleplayer", "singleplayer"),
        idea("Chaos", "chaos"),
        idea("Movement (wall run, slide)", "movement"),
        idea("First person body", "first person"),
        idea("Third person", "third person"),
        idea("VR", "vrmod"),
        idea("Player abilities and powers", "powers"),
        idea("Pill packs", "pill pack"),
        idea("Food and drinks", "drinks"),
        idea("Drugs and cigarettes", "cigarette"),
        idea("Money and printers", "printer"),
        idea("Police and SWAT", "swat"),
        idea("Medic and healing", "medic"),
        idea("Casino", "casino"),
        idea("Pets", "pets"),
    ]),
];

/// Ideas whose name or search phrase contains `filter`, by category.
pub fn matching(filter: &str) -> Vec<(&'static str, Vec<&'static Idea>)> {
    let filter = filter.trim().to_lowercase();
    IDEAS
        .iter()
        .map(|(category, ideas)| {
            let hits: Vec<&Idea> = ideas
                .iter()
                .filter(|idea| idea.name.to_lowercase().contains(&filter) || idea.search.contains(&filter))
                .collect();
            (*category, hits)
        })
        .filter(|(_, hits)| !hits.is_empty())
        .collect()
}

/// The idea with this name, if the text is one.
pub fn named(name: &str) -> Option<&'static Idea> {
    IDEAS.iter().flat_map(|(_, ideas)| ideas.iter()).find(|idea| idea.name.eq_ignore_ascii_case(name.trim()))
}

pub fn count() -> usize {
    IDEAS.iter().map(|(_, ideas)| ideas.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_matches_names_and_phrases() {
        assert_eq!(matching("").len(), IDEAS.len());
        let hits = matching("ZOMB");
        assert!(hits.iter().any(|(_, ideas)| ideas.iter().any(|i| i.name == "Zombie Survival")));
        assert!(matching("no such thing at all").is_empty());
    }

    #[test]
    fn popular_ideas_exist_and_names_are_unique() {
        for name in POPULAR {
            assert!(named(name).is_some(), "{name} is not an idea");
        }
        let mut names: Vec<&str> = IDEAS.iter().flat_map(|(_, ideas)| ideas.iter().map(|i| i.name)).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before);
    }
}
