//! The bundled symbols: Phosphor icons (MIT, see README) in their filled
//! style, sorted into categories with words to find them by. The SVGs live
//! in assets/shapes, named after `Symbol::file`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Animals,
    Plants,
    Sky,
    Food,
    Celebrations,
    Sport,
    Travel,
    Home,
    Music,
    People,
    Symbols,
}

use Category::*;

impl Category {
    pub const ALL: [Category; 11] = [Animals, Plants, Sky, Food, Celebrations, Sport, Travel, Home, Music, People, Symbols];

    pub fn name(self) -> &'static str {
        match self {
            Animals => t!("Animals"),
            Plants => t!("Plants"),
            Sky => t!("Sky & weather"),
            Food => t!("Food & drink"),
            Celebrations => t!("Celebrations"),
            Sport => t!("Sport & games"),
            Travel => t!("Travel"),
            Home => t!("Home & hobbies"),
            Music => t!("Music"),
            People => t!("People & faces"),
            Symbols => t!("Signs & symbols"),
        }
    }
}

#[derive(Debug)]
pub struct Symbol {
    /// The SVG's name without ".svg".
    pub file: &'static str,
    /// English name, translated where shown.
    pub name: &'static str,
    pub category: Category,
    /// English words it can be found by, from Phosphor's own tags.
    pub tags: &'static str,
}

impl Symbol {
    /// Whether a search finds it: by its name in English or the language in
    /// use, its file name, its category or its tags. `needle` is lowercase.
    pub fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || crate::i18n::tr(self.name).to_lowercase().contains(needle)
            || self.name.to_lowercase().contains(needle)
            || self.file.contains(needle)
            || self.category.name().to_lowercase().contains(needle)
            || self.tags.split(' ').any(|t| t.starts_with(needle))
    }
}

pub fn find(file: &str) -> Option<&'static Symbol> {
    SYMBOLS.iter().find(|s| s.file == file)
}

/// The narrowest a stitched part should be, in mm: thinner satin columns
/// and fills don't hold their shape.
const THINNEST_MM: f32 = 1.2;

/// About how small, in mm across, a symbol can be stitched before its thin
/// parts get too narrow: the size where shaving off everything thinner than
/// `THINNEST_MM` still keeps nearly all of it.
pub fn smallest_mm(svg: &str) -> Option<f32> {
    const PX: usize = 160;
    let svg = svg.replace("currentColor", "#000000");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pm = resvg::tiny_skia::Pixmap::new(PX as u32, PX as u32)?;
    let s = PX as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
    let inside: Vec<bool> = pm.pixels().iter().map(|p| p.alpha() > 127).collect();
    let total = inside.iter().filter(|&&b| b).count();
    if total == 0 {
        return None;
    }
    // Distance from each inside pixel to the outside.
    let depth = distance(&inside, PX);
    // The share of the shape kept when parts narrower than 2r pixels go:
    // what lies within r of a pixel at least r deep.
    let kept = |r: f32| {
        let core: Vec<bool> = depth.iter().map(|&d| d >= r).collect();
        let away = distance(&core.iter().map(|&c| !c).collect::<Vec<_>>(), PX);
        inside.iter().zip(&away).filter(|&(&i, &a)| i && a <= r + 0.5).count() as f32 / total as f32
    };
    // The drawing is PX pixels across; at `mm` across, the thinnest part
    // allowed is THINNEST_MM / mm * PX pixels wide.
    let ok = |mm: f32| kept(THINNEST_MM / mm * PX as f32 / 2.0) >= 0.92;
    let (mut lo, mut hi) = (5.0f32, 300.0f32);
    if ok(lo) {
        return Some(lo);
    }
    if !ok(hi) {
        return Some(hi);
    }
    while hi - lo > 1.0 {
        let mid = (lo + hi) / 2.0;
        if ok(mid) { hi = mid } else { lo = mid }
    }
    Some(hi.ceil())
}

/// Chamfer distance (in pixels) from each set pixel to the nearest unset
/// one; unset pixels are 0. Pixels past the edge count as unset.
fn distance(set: &[bool], n: usize) -> Vec<f32> {
    const A: f32 = 1.0;
    const B: f32 = std::f32::consts::SQRT_2;
    let mut d: Vec<f32> = set.iter().map(|&s| if s { f32::MAX } else { 0.0 }).collect();
    let at = |d: &[f32], x: isize, y: isize| if x < 0 || y < 0 || x >= n as isize || y >= n as isize { 0.0 } else { d[y as usize * n + x as usize] };
    for y in 0..n as isize {
        for x in 0..n as isize {
            let i = y as usize * n + x as usize;
            if d[i] > 0.0 {
                let v = (at(&d, x - 1, y) + A).min(at(&d, x, y - 1) + A).min(at(&d, x - 1, y - 1) + B).min(at(&d, x + 1, y - 1) + B);
                d[i] = d[i].min(v);
            }
        }
    }
    for y in (0..n as isize).rev() {
        for x in (0..n as isize).rev() {
            let i = y as usize * n + x as usize;
            if d[i] > 0.0 {
                let v = (at(&d, x + 1, y) + A).min(at(&d, x, y + 1) + A).min(at(&d, x + 1, y + 1) + B).min(at(&d, x - 1, y + 1) + B);
                d[i] = d[i].min(v);
            }
        }
    }
    d
}

pub const SYMBOLS: [Symbol; 291] = [
    Symbol { file: "bird", name: n!("Bird"), category: Animals, tags: "animals pets" },
    Symbol { file: "butterfly", name: n!("Butterfly"), category: Animals, tags: "animals insects moth spring" },
    Symbol { file: "bug", name: n!("Bug"), category: Animals, tags: "debug errors insect ladybug halloween" },
    Symbol { file: "bug-beetle", name: n!("Beetle"), category: Animals, tags: "debug errors insect ladybug" },
    Symbol { file: "cat", name: n!("Cat"), category: Animals, tags: "pets animals kitty kitten" },
    Symbol { file: "cow", name: n!("Cow"), category: Animals, tags: "animals livestock beef bull milk dairy farm" },
    Symbol { file: "dog", name: n!("Dog"), category: Animals, tags: "pets animals puppy" },
    Symbol { file: "fish", name: n!("Fish"), category: Animals, tags: "animals pets food seafood restaurants dining sea" },
    Symbol { file: "fish-simple", name: n!("Simple fish"), category: Animals, tags: "animals pets food seafood restaurants dining sea" },
    Symbol { file: "horse", name: n!("Horse"), category: Animals, tags: "animals equestrian chess knight sports farm" },
    Symbol { file: "rabbit", name: n!("Rabbit"), category: Animals, tags: "hare fast animal easter baby" },
    Symbol { file: "paw-print", name: n!("Paw print"), category: Animals, tags: "pets pet store pet shop animals cat dog veterinarian" },
    Symbol { file: "shrimp", name: n!("Shrimp"), category: Animals, tags: "food seafood prawn sea" },
    Symbol { file: "egg", name: n!("Egg"), category: Animals, tags: "chicken food meal baby hatch easter farm" },
    Symbol { file: "egg-crack", name: n!("Cracked egg"), category: Animals, tags: "chicken food meal baby hatch break easter" },
    Symbol { file: "bone", name: n!("Bone"), category: Animals, tags: "dogbone" },
    Symbol { file: "feather", name: n!("Feather"), category: Animals, tags: "bird" },
    Symbol { file: "acorn", name: n!("Acorn"), category: Plants, tags: "savings nut vegetable veggies food groceries market autumn" },
    Symbol { file: "cactus", name: n!("Cactus"), category: Plants, tags: "plants cacti desert western" },
    Symbol { file: "clover", name: n!("Clover"), category: Plants, tags: "four leaf clover plants luck lucky irish spring" },
    Symbol { file: "flower", name: n!("Flower"), category: Plants, tags: "plants green environmental wedding spring" },
    Symbol { file: "flower-lotus", name: n!("Lotus"), category: Plants, tags: "plants green environmental spirituality" },
    Symbol { file: "flower-tulip", name: n!("Tulip"), category: Plants, tags: "plants green environmental easter spring" },
    Symbol { file: "leaf", name: n!("Leaf"), category: Plants, tags: "plants trees branches leaves nodes green environmental autumn" },
    Symbol { file: "plant", name: n!("Sprout"), category: Plants, tags: "sprout vegan farm farming garden gardening" },
    Symbol { file: "potted-plant", name: n!("Potted plant"), category: Plants, tags: "sprout vegan garden gardening" },
    Symbol { file: "tree", name: n!("Tree"), category: Plants, tags: "plants branches leaves green environmental" },
    Symbol { file: "tree-evergreen", name: n!("Fir tree"), category: Plants, tags: "plants branches leaves pine conifer green environmental christmas winter" },
    Symbol { file: "tree-palm", name: n!("Palm tree"), category: Plants, tags: "tropical beach plants branches leaves green environmental summer holiday" },
    Symbol { file: "grains", name: n!("Wheat"), category: Plants, tags: "wheat gluten farm farming agriculture farm" },
    Symbol { file: "log", name: n!("Log"), category: Plants, tags: "tree stump stick branch" },
    Symbol { file: "park", name: n!("Park"), category: Plants, tags: "bench outdoors" },
    Symbol { file: "cloud", name: n!("Cloud"), category: Sky, tags: "serverless backup storage meteorology cloudy overcast" },
    Symbol { file: "cloud-lightning", name: n!("Storm"), category: Sky, tags: "meteorology cloudy overcast stormy thunderstorm" },
    Symbol { file: "cloud-moon", name: n!("Cloudy night"), category: Sky, tags: "meteorology cloudy partly cloudy night evening" },
    Symbol { file: "cloud-rain", name: n!("Rain"), category: Sky, tags: "meteorology cloudy rainy raining stormy rainstorm" },
    Symbol { file: "cloud-snow", name: n!("Snow"), category: Sky, tags: "meteorology cloudy snowy snowing stormy snowstorm christmas winter" },
    Symbol { file: "cloud-sun", name: n!("Sun and cloud"), category: Sky, tags: "meteorology cloudy partly cloudy partly sunny" },
    Symbol {
        file: "lightning",
        name: n!("Lightning"),
        category: Sky,
        tags: "meteorology stormy thunderstorm thunderbolt charged charger charging power electricity flash",
    },
    Symbol { file: "moon", name: n!("Moon"), category: Sky, tags: "night evening clear sleep snooze night mode dark mode astronomy stargazing" },
    Symbol {
        file: "moon-stars",
        name: n!("Moon and stars"),
        category: Sky,
        tags: "night evening clear sleep snooze night mode dark mode astronomy stargazing constellation halloween baby",
    },
    Symbol { file: "rainbow", name: n!("Rainbow"), category: Sky, tags: "meteorology rainstorm arc pride lgbtq+ leprechaun" },
    Symbol {
        file: "rainbow-cloud",
        name: n!("Rainbow cloud"),
        category: Sky,
        tags: "meteorology rainstorm cloudy partly cloudy partly sunny pride lgbtq+ leprechaun",
    },
    Symbol { file: "snowflake", name: n!("Snowflake"), category: Sky, tags: "meteorology snowy snowing snowstorm christmas winter" },
    Symbol {
        file: "sun",
        name: n!("Sun"),
        category: Sky,
        tags: "day daytime daylight clear sunny sunshine light mode brightness lighten brighten summer holiday",
    },
    Symbol { file: "sun-horizon", name: n!("Sunrise"), category: Sky, tags: "day daytime daylight clear sunny sunshine sunrise sunset" },
    Symbol { file: "umbrella", name: n!("Umbrella"), category: Sky, tags: "raining rainy insurance" },
    Symbol { file: "wind", name: n!("Wind"), category: Sky, tags: "meteorology windy stormy blustery gusty air" },
    Symbol { file: "tornado", name: n!("Tornado"), category: Sky, tags: "meteorology twister storm wind disaster" },
    Symbol { file: "drop", name: n!("Drop"), category: Sky, tags: "droplet teardrop raindrop raining meteorology water blur" },
    Symbol { file: "waves", name: n!("Waves"), category: Sky, tags: "ocean tides surf summer holiday sea" },
    Symbol { file: "mountains", name: n!("Mountains"), category: Sky, tags: "hills outdoors terrain geology adventure" },
    Symbol { file: "island", name: n!("Island"), category: Sky, tags: "geography beach ocean tropical palm vacation trip locations places summer holiday" },
    Symbol { file: "planet", name: n!("Planet"), category: Sky, tags: "saturn world globe astronomy space" },
    Symbol { file: "meteor", name: n!("Meteor"), category: Sky, tags: "astronomy asteroid fireball" },
    Symbol { file: "shooting-star", name: n!("Shooting star"), category: Sky, tags: "wish comet meteor" },
    Symbol { file: "flying-saucer", name: n!("Flying saucer"), category: Sky, tags: "ufo space aliens extra terrestrial sci-fi" },
    Symbol { file: "avocado", name: n!("Avocado"), category: Food, tags: "food vegetable veggie fruit groceries market" },
    Symbol { file: "bread", name: n!("Bread"), category: Food, tags: "food meal bakery sandwich gluten loaf toast slice" },
    Symbol { file: "cake", name: n!("Cake"), category: Food, tags: "dessert birthday celebration event birthday" },
    Symbol { file: "carrot", name: n!("Carrot"), category: Food, tags: "food vegetable veggie groceries market" },
    Symbol { file: "cheese", name: n!("Cheese"), category: Food, tags: "dairy wedge food dining" },
    Symbol { file: "cherries", name: n!("Cherries"), category: Food, tags: "food fruit cherry groceries market" },
    Symbol {
        file: "coffee",
        name: n!("Coffee"),
        category: Food,
        tags: "tea java beverages drinks cafe cup mug espresso cappuccino latte places locations bars restaurants food dining kitchen",
    },
    Symbol {
        file: "coffee-bean",
        name: n!("Coffee bean"),
        category: Food,
        tags: "tea java beverages drinks cafe cup mug espresso cappuccino latte places locations bars restaurants food dining",
    },
    Symbol { file: "cookie", name: n!("Cookie"), category: Food, tags: "privacy dessert food dining" },
    Symbol { file: "hamburger", name: n!("Burger"), category: Food, tags: "fast food party places locations restaurants food dining" },
    Symbol { file: "ice-cream", name: n!("Ice cream"), category: Food, tags: "food dessert cone summer holiday" },
    Symbol { file: "orange", name: n!("Orange"), category: Food, tags: "food fruit juice citrus groceries market" },
    Symbol { file: "orange-slice", name: n!("Orange slice"), category: Food, tags: "food fruit juice citrus groceries market" },
    Symbol { file: "pepper", name: n!("Pepper"), category: Food, tags: "food spicy chile vegetable" },
    Symbol { file: "pizza", name: n!("Pizza"), category: Food, tags: "fast food party places locations restaurants food dining" },
    Symbol { file: "popcorn", name: n!("Popcorn"), category: Food, tags: "food movies theater" },
    Symbol { file: "popsicle", name: n!("Ice lolly"), category: Food, tags: "ice cream dessert summer food summer holiday" },
    Symbol { file: "onigiri", name: n!("Rice ball"), category: Food, tags: "rice ball japanese food restaurant" },
    Symbol { file: "bowl-steam", name: n!("Hot bowl"), category: Food, tags: "food meal eating restaurants dining locations" },
    Symbol { file: "cooking-pot", name: n!("Pot"), category: Food, tags: "stew kitchen steaming restaurants food dining kitchen" },
    Symbol { file: "fork-knife", name: n!("Fork and knife"), category: Food, tags: "food meal eating restaurants dining utensils kitchen" },
    Symbol { file: "wine", name: n!("Wine"), category: Food, tags: "drinks beverages vineyard places locations bars restaurants food dining" },
    Symbol { file: "beer-stein", name: n!("Beer mug"), category: Food, tags: "drinks beverages places locations bars restaurants food dining" },
    Symbol {
        file: "champagne",
        name: n!("Champagne"),
        category: Food,
        tags: "glass drinks beverages wine places locations bars restaurants food dining wedding",
    },
    Symbol { file: "martini", name: n!("Cocktail"), category: Food, tags: "glass drinks beverages cocktails places locations bars restaurants food dining" },
    Symbol { file: "tea-bag", name: n!("Tea bag"), category: Food, tags: "drinks beverages sachet caffeine places locations bars restaurants food dining" },
    Symbol { file: "pint-glass", name: n!("Glass"), category: Food, tags: "water beverage drinks beverages food dining" },
    Symbol { file: "chef-hat", name: n!("Chef's hat"), category: Food, tags: "cooking cuisine kitchen clothes clothing kitchen" },
    Symbol { file: "balloon", name: n!("Balloon"), category: Celebrations, tags: "helium birthday party birthday" },
    Symbol { file: "confetti", name: n!("Confetti"), category: Celebrations, tags: "tada party emoji birthday" },
    Symbol { file: "gift", name: n!("Gift"), category: Celebrations, tags: "presents holiday birthday christmas birthday" },
    Symbol {
        file: "cheers",
        name: n!("Cheers"),
        category: Celebrations,
        tags: "glass drinks beverages champagne toast places locations bars restaurants food dining wedding",
    },
    Symbol {
        file: "bell",
        name: n!("Bell"),
        category: Celebrations,
        tags: "alarm notifications times timer clock schedule events ringer calls christmas wedding",
    },
    Symbol { file: "pinwheel", name: n!("Pinwheel"), category: Celebrations, tags: "toys whirligig" },
    Symbol { file: "disco-ball", name: n!("Disco ball"), category: Celebrations, tags: "danging club 70s" },
    Symbol {
        file: "graduation-cap",
        name: n!("Graduation cap"),
        category: Celebrations,
        tags: "classroom teacher education school college university degree graduate hat school",
    },
    Symbol { file: "mask-happy", name: n!("Happy mask"), category: Celebrations, tags: "theater costume smile smiling thalia" },
    Symbol { file: "mask-sad", name: n!("Sad mask"), category: Celebrations, tags: "theater costume cry crying melpomene" },
    Symbol { file: "ticket", name: n!("Ticket"), category: Celebrations, tags: "ticketstub movie ticket entry admissions events" },
    Symbol { file: "sparkle", name: n!("Sparkle"), category: Celebrations, tags: "star rate ratings favorites favorited christmas" },
    Symbol { file: "star-four", name: n!("Four-point star"), category: Celebrations, tags: "rate ratings favorites favorited christmas" },
    Symbol { file: "baseball", name: n!("Baseball"), category: Sport, tags: "sports mlb" },
    Symbol { file: "basketball", name: n!("Basketball"), category: Sport, tags: "sports nba" },
    Symbol { file: "beach-ball", name: n!("Beach ball"), category: Sport, tags: "sports ocean party summer holiday" },
    Symbol { file: "bowling-ball", name: n!("Bowling ball"), category: Sport, tags: "sports alley" },
    Symbol { file: "boxing-glove", name: n!("Boxing glove"), category: Sport, tags: "sports combat martial arts fight gym" },
    Symbol { file: "football", name: n!("American football"), category: Sport, tags: "sports american football nfl" },
    Symbol { file: "football-helmet", name: n!("Football helmet"), category: Sport, tags: "sports american football nfl" },
    Symbol { file: "golf", name: n!("Golf"), category: Sport, tags: "sports pga tee ball" },
    Symbol { file: "hockey", name: n!("Hockey"), category: Sport, tags: "sports nhl stick puck" },
    Symbol { file: "ping-pong", name: n!("Table tennis"), category: Sport, tags: "sports paddle" },
    Symbol { file: "racquet", name: n!("Racquet"), category: Sport, tags: "sports tennis squash pickleball" },
    Symbol { file: "soccer-ball", name: n!("Football"), category: Sport, tags: "sports football mls" },
    Symbol { file: "tennis-ball", name: n!("Tennis ball"), category: Sport, tags: "sports mlb" },
    Symbol { file: "volleyball", name: n!("Volleyball"), category: Sport, tags: "sports" },
    Symbol { file: "trophy", name: n!("Trophy"), category: Sport, tags: "ribbons medals winning victory awards prize" },
    Symbol { file: "medal", name: n!("Medal"), category: Sport, tags: "ribbons winning victory awards rosette" },
    Symbol { file: "barbell", name: n!("Barbell"), category: Sport, tags: "gym weights dumbbells strength training workout exercises fitness" },
    Symbol { file: "sneaker", name: n!("Sneaker"), category: Sport, tags: "clothes clothing shoes trainers sports" },
    Symbol {
        file: "person-simple-run",
        name: n!("Runner"),
        category: Sport,
        tags: "pedestrian running human woman man body transit transportation travel commuter user exercise",
    },
    Symbol { file: "person-simple-swim", name: n!("Swimmer"), category: Sport, tags: "pedestrian swimming human woman man body sports exercise" },
    Symbol { file: "person-simple-bike", name: n!("Cyclist"), category: Sport, tags: "pedestrian biking bicycle human woman man body sports exercise" },
    Symbol { file: "person-simple-ski", name: n!("Skier"), category: Sport, tags: "sports skiing winter mountain winter" },
    Symbol { file: "person-simple-snowboard", name: n!("Snowboarder"), category: Sport, tags: "sports snowboarding winter mountain winter" },
    Symbol { file: "person-simple-hike", name: n!("Hiker"), category: Sport, tags: "pedestrian climbing mountaineering bouldering" },
    Symbol { file: "dice-five", name: n!("Dice five"), category: Sport, tags: "die rolling gamble gambling casino gaming 5" },
    Symbol { file: "dice-six", name: n!("Dice six"), category: Sport, tags: "die rolling gamble gambling casino gaming 6" },
    Symbol { file: "puzzle-piece", name: n!("Puzzle piece"), category: Sport, tags: "board game element component extension plugin" },
    Symbol {
        file: "game-controller",
        name: n!("Game controller"),
        category: Sport,
        tags: "gaming video games nintendo switch sony playstation microsoft xbox",
    },
    Symbol { file: "joystick", name: n!("Joystick"), category: Sport, tags: "gaming game controller video games arcade game atari" },
    Symbol { file: "club", name: n!("Clubs"), category: Sport, tags: "clubs suits cards gambling casino gaming" },
    Symbol { file: "spade", name: n!("Spades"), category: Sport, tags: "spades suits cards gambling casino gaming" },
    Symbol { file: "checkerboard", name: n!("Checkerboard"), category: Sport, tags: "crossword" },
    Symbol { file: "flag-checkered", name: n!("Checkered flag"), category: Sport, tags: "flags race racing finish line" },
    Symbol { file: "sword", name: n!("Sword"), category: Sport, tags: "weapon knife dagger gladius video games rpg gaming war" },
    Symbol { file: "castle-turret", name: n!("Castle"), category: Sport, tags: "chess rook" },
    Symbol { file: "treasure-chest", name: n!("Treasure chest"), category: Sport, tags: "loot lootbox inventory rewards gaming pirate" },
    Symbol { file: "airplane", name: n!("Airplane"), category: Travel, tags: "vehicles airports flights flying planes transit transportation traveling" },
    Symbol {
        file: "airplane-tilt",
        name: n!("Plane in flight"),
        category: Travel,
        tags: "vehicles airports flights flying planes transit transportation traveling departure",
    },
    Symbol { file: "anchor", name: n!("Anchor"), category: Travel, tags: "nautical boats ships hope safety insurance sea" },
    Symbol {
        file: "boat",
        name: n!("Boat"),
        category: Travel,
        tags: "ferry ship cruise vehicles public transit transportation commuter traveling sailing places locations sea",
    },
    Symbol {
        file: "sailboat",
        name: n!("Sailboat"),
        category: Travel,
        tags: "sailing ship vehicles transportation sports places locations summer holiday sea",
    },
    Symbol { file: "bicycle", name: n!("Bicycle"), category: Travel, tags: "bikers bicycling cyclists transit transportation commuter exercises fitness" },
    Symbol { file: "bus", name: n!("Bus"), category: Travel, tags: "vehicles automobile public transit transportation commuter traveling places locations" },
    Symbol { file: "car", name: n!("Car"), category: Travel, tags: "cars vehicles automobile transit transportation traveling" },
    Symbol { file: "car-profile", name: n!("Car side"), category: Travel, tags: "cars vehicles automobile transit transportation traveling" },
    Symbol { file: "jeep", name: n!("Jeep"), category: Travel, tags: "vehicles automobile suv cars trucks wrangler off-road transit transportation traveling" },
    Symbol { file: "motorcycle", name: n!("Motorbike"), category: Travel, tags: "vehicles motorbike transportation travel" },
    Symbol { file: "scooter", name: n!("Scooter"), category: Travel, tags: "vehicles transportation travel transit" },
    Symbol { file: "rocket", name: n!("Rocket"), category: Travel, tags: "spaceship launch deployment rocketship" },
    Symbol { file: "rocket-launch", name: n!("Rocket launch"), category: Travel, tags: "spaceship flying blastoff deployment rocketship" },
    Symbol {
        file: "train",
        name: n!("Train"),
        category: Travel,
        tags: "vehicles subway light rail public transit transportation commuter traveling places locations",
    },
    Symbol { file: "tractor", name: n!("Tractor"), category: Travel, tags: "vehicles farmer field farming agriculture farm" },
    Symbol { file: "truck", name: n!("Truck"), category: Travel, tags: "trucks cars vehicles automobile shipping delivery" },
    Symbol { file: "van", name: n!("Van"), category: Travel, tags: "westfalia microbus vanagon cars vehicles automobile transit transportation traveling" },
    Symbol { file: "taxi", name: n!("Taxi"), category: Travel, tags: "vehicles cars automobiles livery limousine uber" },
    Symbol { file: "police-car", name: n!("Police car"), category: Travel, tags: "vehicles cars automobiles law enforcement" },
    Symbol { file: "fire-truck", name: n!("Fire engine"), category: Travel, tags: "first-aid emt medical medicine injury safety emergency firefighter" },
    Symbol { file: "ambulance", name: n!("Ambulance"), category: Travel, tags: "first-aid emt medical medicine injury safety emergency doctor vehicles" },
    Symbol {
        file: "globe",
        name: n!("Globe"),
        category: Travel,
        tags: "world earth global planet circle round internationalization i18n languages country countries geography internet",
    },
    Symbol {
        file: "globe-hemisphere-west",
        name: n!("Earth"),
        category: Travel,
        tags: "world earth global planet circle round internationalization i18n languages country countries geography north america south america",
    },
    Symbol { file: "compass", name: n!("Compass"), category: Travel, tags: "navigation directions maps safari apple" },
    Symbol { file: "compass-rose", name: n!("Compass rose"), category: Travel, tags: "navigation directions maps cardinal cartography" },
    Symbol { file: "map-pin", name: n!("Map pin"), category: Travel, tags: "maps places markers pins locations poi point of interest" },
    Symbol { file: "lighthouse", name: n!("Lighthouse"), category: Travel, tags: "buildings nautical navigation boats ships safety sea" },
    Symbol { file: "tent", name: n!("Tent"), category: Travel, tags: "camping outdoors tarp summer holiday" },
    Symbol { file: "tipi", name: n!("Tipi"), category: Travel, tags: "teepee lodge tent outdoors camping tarp" },
    Symbol { file: "campfire", name: n!("Campfire"), category: Travel, tags: "camping flame bonfire outdoors" },
    Symbol { file: "suitcase-rolling", name: n!("Suitcase"), category: Travel, tags: "suitcase luggage travel transportation" },
    Symbol { file: "paper-plane-tilt", name: n!("Paper plane"), category: Travel, tags: "mail email send sent messages messaging toys games" },
    Symbol { file: "binoculars", name: n!("Binoculars"), category: Travel, tags: "telescope glasses search find explore" },
    Symbol { file: "signpost", name: n!("Signpost"), category: Travel, tags: "direction traffic road sign transit transportation" },
    Symbol { file: "bridge", name: n!("River bridge"), category: Travel, tags: "travel transportation infrastucture" },
    Symbol { file: "house", name: n!("House"), category: Home, tags: "homes buildings places locations" },
    Symbol { file: "barn", name: n!("Barn"), category: Home, tags: "animals livestock buildings farming agriculture farm" },
    Symbol { file: "church", name: n!("Church"), category: Home, tags: "christ christianity cathedral religion worship" },
    Symbol { file: "mosque", name: n!("Mosque"), category: Home, tags: "islam muslim religion worship" },
    Symbol { file: "synagogue", name: n!("Synagogue"), category: Home, tags: "jewish judaism temple religion worship" },
    Symbol { file: "key", name: n!("Key"), category: Home, tags: "padlock security secured authentication authenticated login locked encrypted encryption" },
    Symbol {
        file: "lock",
        name: n!("Padlock"),
        category: Home,
        tags: "padlock security secured authentication authenticated login locked encrypted encryption privacy private",
    },
    Symbol { file: "lamp", name: n!("Lamp"), category: Home, tags: "light furniture appliances" },
    Symbol { file: "armchair", name: n!("Armchair"), category: Home, tags: "seat furniture" },
    Symbol { file: "bathtub", name: n!("Bathtub"), category: Home, tags: "bath shower bathroom faucet" },
    Symbol { file: "needle", name: n!("Needle"), category: Home, tags: "sewing thread awl tailor sewing" },
    Symbol { file: "yarn", name: n!("Yarn"), category: Home, tags: "knitting sewing sewing" },
    Symbol { file: "scissors", name: n!("Scissors"), category: Home, tags: "cut snip clipboard sewing" },
    Symbol { file: "paint-brush", name: n!("Paintbrush"), category: Home, tags: "colors color picker arts" },
    Symbol { file: "palette", name: n!("Palette"), category: Home, tags: "paint colors color picker arts" },
    Symbol { file: "pencil", name: n!("Pencil"), category: Home, tags: "write writing editing sign signature school" },
    Symbol { file: "swatches", name: n!("Swatches"), category: Home, tags: "colors color picker samples pantone sewing" },
    Symbol { file: "camera", name: n!("Camera"), category: Home, tags: "photography pictures lens" },
    Symbol { file: "book-open", name: n!("Open book"), category: Home, tags: "reading reader novel story library school" },
    Symbol { file: "books", name: n!("Books"), category: Home, tags: "reading reader bookshelf library places locations school" },
    Symbol { file: "hammer", name: n!("Hammer"), category: Home, tags: "tools carpentry construction" },
    Symbol { file: "wrench", name: n!("Wrench"), category: Home, tags: "settings setup preferences tools machinery mechanical repairs" },
    Symbol { file: "axe", name: n!("Axe"), category: Home, tags: "tools carpentry forestry construction" },
    Symbol { file: "shovel", name: n!("Shovel"), category: Home, tags: "tools digging construction" },
    Symbol { file: "magic-wand", name: n!("Magic wand"), category: Home, tags: "selection wizard games" },
    Symbol { file: "eyeglasses", name: n!("Glasses"), category: Home, tags: "vision spectacles" },
    Symbol { file: "sunglasses", name: n!("Sunglasses"), category: Home, tags: "vision sun spectacles summer holiday" },
    Symbol { file: "hourglass", name: n!("Hourglass"), category: Home, tags: "times timer alarms clock schedule events waiting progress" },
    Symbol { file: "clock", name: n!("Clock"), category: Home, tags: "times timer alarm schedule events watch" },
    Symbol { file: "lightbulb", name: n!("Light bulb"), category: Home, tags: "flashlight bulbs lighting led energy idea" },
    Symbol { file: "t-shirt", name: n!("T-shirt"), category: Home, tags: "clothes clothing sewing" },
    Symbol { file: "dress", name: n!("Dress"), category: Home, tags: "clothes clothing sewing" },
    Symbol { file: "sock", name: n!("Sock"), category: Home, tags: "clothes clothing winter" },
    Symbol { file: "baseball-cap", name: n!("Baseball cap"), category: Home, tags: "clothes clothing sports hat" },
    Symbol { file: "cowboy-hat", name: n!("Cowboy hat"), category: Home, tags: "clothes clothing stetson" },
    Symbol { file: "beanie", name: n!("Beanie"), category: Home, tags: "clothes clothing sports hat winter winter" },
    Symbol { file: "high-heel", name: n!("High heel"), category: Home, tags: "pumps clothes clothing shoes" },
    Symbol { file: "handbag", name: n!("Handbag"), category: Home, tags: "suitcases valises baggage purses" },
    Symbol { file: "coat-hanger", name: n!("Coat hanger"), category: Home, tags: "clothing clothes closet sewing" },
    Symbol { file: "backpack", name: n!("Backpack"), category: Home, tags: "knapsack camping school bag school" },
    Symbol { file: "basket", name: n!("Basket"), category: Home, tags: "ecommerce market cart buying shopping groceries checkout places locations easter" },
    Symbol { file: "guitar", name: n!("Guitar"), category: Music, tags: "music instrument" },
    Symbol { file: "headphones", name: n!("Headphones"), category: Music, tags: "music audio listening" },
    Symbol { file: "microphone-stage", name: n!("Microphone"), category: Music, tags: "audio recording music sound performance concert" },
    Symbol { file: "music-note", name: n!("Music note"), category: Music, tags: "songs audio playlist albums" },
    Symbol { file: "music-notes", name: n!("Music notes"), category: Music, tags: "songs audio playlist albums" },
    Symbol { file: "piano-keys", name: n!("Piano keys"), category: Music, tags: "music instrument keyboard" },
    Symbol { file: "vinyl-record", name: n!("Record"), category: Music, tags: "recording audio album music ep lp" },
    Symbol { file: "metronome", name: n!("Metronome"), category: Music, tags: "music beat tempo bpm" },
    Symbol { file: "film-slate", name: n!("Clapperboard"), category: Music, tags: "clapper movie" },
    Symbol { file: "radio", name: n!("Radio"), category: Music, tags: "broadcast fm am xm transmitter receiver" },
    Symbol { file: "baby", name: n!("Baby"), category: People, tags: "infant child children toddler" },
    Symbol { file: "baby-carriage", name: n!("Pram"), category: People, tags: "pram stroller infant child children toddler baby" },
    Symbol { file: "hand-heart", name: n!("Hand with heart"), category: People, tags: "donation care emoji wedding valentine love" },
    Symbol { file: "hand-peace", name: n!("Peace hand"), category: People, tags: "emoji victory" },
    Symbol { file: "hand-waving", name: n!("Waving hand"), category: People, tags: "emoji palm wave hello goodbye" },
    Symbol { file: "hands-clapping", name: n!("Clapping hands"), category: People, tags: "emoji clap applause" },
    Symbol { file: "hands-praying", name: n!("Praying hands"), category: People, tags: "emoji religion worship prayer thanks thank you" },
    Symbol { file: "handshake", name: n!("Handshake"), category: People, tags: "emoji deal agreement" },
    Symbol { file: "thumbs-up", name: n!("Thumbs up"), category: People, tags: "like love favorited favorites emoji yes" },
    Symbol { file: "hand-fist", name: n!("Fist"), category: People, tags: "emoji power protest blm" },
    Symbol { file: "smiley", name: n!("Smiley"), category: People, tags: "face emoji happy grinning smiling" },
    Symbol { file: "smiley-wink", name: n!("Winking smiley"), category: People, tags: "face emoji winking flirting cute" },
    Symbol { file: "smiley-sad", name: n!("Sad smiley"), category: People, tags: "face emoji unhappy frowning" },
    Symbol { file: "smiley-meh", name: n!("Meh smiley"), category: People, tags: "face emoji unimpressed neutral" },
    Symbol { file: "smiley-melting", name: n!("Melting smiley"), category: People, tags: "face emoji embarrassment sarcasm shame hot heat" },
    Symbol { file: "smiley-x-eyes", name: n!("Dizzy smiley"), category: People, tags: "face emoji dead killed unconscious" },
    Symbol { file: "ghost", name: n!("Ghost"), category: People, tags: "pac-man spirit scary halloween" },
    Symbol { file: "skull", name: n!("Skull"), category: People, tags: "death dead kill halloween" },
    Symbol { file: "alien", name: n!("Alien"), category: People, tags: "ufo space flying saucer extra terrestrial sci-fi halloween" },
    Symbol { file: "robot", name: n!("Robot"), category: People, tags: "automaton artificial intelligence ai" },
    Symbol { file: "person", name: n!("Person"), category: People, tags: "walking human woman man body transit transportation travel commuter user" },
    Symbol { file: "person-arms-spread", name: n!("Person with open arms"), category: People, tags: "accessibility vitruvian stretch sports" },
    Symbol { file: "footprints", name: n!("Footprints"), category: People, tags: "path trail walk route hike hiking baby" },
    Symbol { file: "eyes", name: n!("Eyes"), category: People, tags: "look glance" },
    Symbol { file: "tooth", name: n!("Tooth"), category: People, tags: "teeth dentistry medical medicine doctor cavity" },
    Symbol { file: "lego-smiley", name: n!("Toy figure"), category: People, tags: "face emoji toys blocks bricks" },
    Symbol {
        file: "heart",
        name: n!("Heart"),
        category: Symbols,
        tags: "wellness love healthy like favorites favorited suits cards gambling casino gaming emoji wedding valentine love",
    },
    Symbol {
        file: "heart-straight",
        name: n!("Straight heart"),
        category: Symbols,
        tags: "wellness love healthy like favorites favorited suits cards gambling casino gaming emoji valentine love",
    },
    Symbol { file: "heart-half", name: n!("Half heart"), category: Symbols, tags: "wellness love healthy like favorites favorited valentine love" },
    Symbol { file: "heart-break", name: n!("Broken heart"), category: Symbols, tags: "love hate crack split divorce emoji" },
    Symbol { file: "star", name: n!("Star"), category: Symbols, tags: "rate ratings favorites favorited christmas" },
    Symbol { file: "star-half", name: n!("Half star"), category: Symbols, tags: "rate ratings" },
    Symbol { file: "crown", name: n!("Crown"), category: Symbols, tags: "king queen royalty monarch ruler leader chess" },
    Symbol { file: "crown-simple", name: n!("Simple crown"), category: Symbols, tags: "king queen royalty monarch ruler leader chess wedding" },
    Symbol { file: "crown-cross", name: n!("Royal crown"), category: Symbols, tags: "king queen royalty monarch ruler leader chess" },
    Symbol {
        file: "shield",
        name: n!("Shield"),
        category: Symbols,
        tags: "badge security secured defense defended authentication authenticated guarded locked encrypted encryption",
    },
    Symbol {
        file: "shield-star",
        name: n!("Shield with star"),
        category: Symbols,
        tags: "badge security defense authentication authenticated guarded locked encrypted encryption",
    },
    Symbol {
        file: "shield-check",
        name: n!("Shield with tick"),
        category: Symbols,
        tags: "badge security defense authentication authenticated guarded locked encrypted encryption",
    },
    Symbol {
        file: "shield-checkered",
        name: n!("Quartered shield"),
        category: Symbols,
        tags: "badge security defense authentication authenticated guarded locked encrypted encryption",
    },
    Symbol { file: "seal", name: n!("Seal"), category: Symbols, tags: "badge verified verification shapes polygons circle-wavy" },
    Symbol { file: "seal-check", name: n!("Seal with tick"), category: Symbols, tags: "badge verified verification shapes polygons circle-wavy" },
    Symbol { file: "peace", name: n!("Peace"), category: Symbols, tags: "love hippies peace sign symbols" },
    Symbol { file: "yin-yang", name: n!("Yin yang"), category: Symbols, tags: "symbol good evil black white" },
    Symbol { file: "infinity", name: n!("Infinity"), category: Symbols, tags: "infinite lemniscate figure-eight mathematics arithmetic calculator ∞" },
    Symbol { file: "cross", name: n!("Cross"), category: Symbols, tags: "dagger crucifix christ christianity religion worship symbol" },
    Symbol { file: "star-of-david", name: n!("Star of David"), category: Symbols, tags: "judaism jewish hexagram religion worship symbol" },
    Symbol { file: "star-and-crescent", name: n!("Star and crescent"), category: Symbols, tags: "islam muslim moon religion worship symbol" },
    Symbol { file: "flag", name: n!("Flag"), category: Symbols, tags: "country countries finished completed flags" },
    Symbol { file: "flag-banner", name: n!("Banner flag"), category: Symbols, tags: "ribbon country countries finished completed flags swallowtail" },
    Symbol { file: "flag-pennant", name: n!("Pennant"), category: Symbols, tags: "flags race sports team" },
    Symbol { file: "flag-banner-fold", name: n!("Ribbon banner"), category: Symbols, tags: "ribbon country countries finished completed flags swallowtail" },
    Symbol { file: "diamond", name: n!("Diamonds"), category: Symbols, tags: "rectangle shapes polygons diamonds suits cards gambling casino gaming" },
    Symbol { file: "diamonds-four", name: n!("Four diamonds"), category: Symbols, tags: "shapes grid component" },
    Symbol { file: "spiral", name: n!("Spiral"), category: Symbols, tags: "spin rotate dizzy" },
    Symbol { file: "asterisk", name: n!("Asterisk"), category: Symbols, tags: "star wildcard bullet point 6 emergency" },
    Symbol { file: "certificate", name: n!("Certificate"), category: Symbols, tags: "awards certification degree diploma" },
    Symbol { file: "target", name: n!("Target"), category: Symbols, tags: "bullseye radar archery accuracy precision" },
    Symbol { file: "circle", name: n!("Circle"), category: Symbols, tags: "round shapes polygons" },
    Symbol { file: "hexagon", name: n!("Hexagon"), category: Symbols, tags: "6 shapes polygons" },
    Symbol { file: "pentagon", name: n!("Pentagon"), category: Symbols, tags: "shapes polygons 5" },
    Symbol { file: "octagon", name: n!("Octagon"), category: Symbols, tags: "8 shapes polygons" },
    Symbol { file: "triangle", name: n!("Triangle"), category: Symbols, tags: "3 shapes polygons" },
    Symbol { file: "square", name: n!("Square"), category: Symbols, tags: "4 shapes polygons box" },
    Symbol { file: "fire", name: n!("Fire"), category: Symbols, tags: "flame burning match lighter" },
    Symbol { file: "flame", name: n!("Flame"), category: Symbols, tags: "fire burning match lighter" },
    Symbol { file: "lifebuoy", name: n!("Lifebuoy"), category: Symbols, tags: "lifebelt lifesaver safety help support nautical boats ships sea" },
    Symbol { file: "recycle", name: n!("Recycle"), category: Symbols, tags: "recycling trash environmental green" },
    Symbol { file: "heartbeat", name: n!("Heartbeat"), category: Symbols, tags: "wellness healthy ecg ekg vitals monitor" },
    Symbol { file: "pentagram", name: n!("Pentagram"), category: Symbols, tags: "star wicca satan satanism religion spirituality evil magic" },
    Symbol { file: "atom", name: n!("Atom"), category: Symbols, tags: "atomic nucleus nuclear reactor science physics electron automation react" },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shapes"))
    }

    #[test]
    fn every_symbol_has_its_file_and_every_file_its_symbol() {
        for s in &SYMBOLS {
            assert!(dir().join(format!("{}.svg", s.file)).is_file(), "{} has no SVG", s.file);
        }
        for e in std::fs::read_dir(dir()).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "svg") {
                let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
                assert!(find(&stem).is_some(), "{stem}.svg isn't in the catalog");
            }
        }
        let mut names: Vec<&str> = SYMBOLS.iter().map(|s| s.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), SYMBOLS.len(), "two symbols share a name");
    }

    #[test]
    fn search_finds_by_name_tag_and_category() {
        let hits = |q: &str| SYMBOLS.iter().filter(|s| s.matches(q)).map(|s| s.file).collect::<Vec<_>>();
        assert!(hits("cat").contains(&"cat"));
        assert!(hits("christmas").contains(&"tree-evergreen") || hits("winter").contains(&"snowflake"));
        assert!(hits("animals").contains(&"dog"));
        assert_eq!(hits("").len(), SYMBOLS.len());
    }

    #[test]
    fn thin_drawings_need_more_room_than_solid_ones() {
        let solid = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="48"/></svg>"#;
        let thin = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle cx="50" cy="50" r="45" fill="none" stroke="black" stroke-width="3"/></svg>"#;
        let (a, b) = (smallest_mm(solid).unwrap(), smallest_mm(thin).unwrap());
        assert!(a <= 10.0, "solid circle needs {a} mm");
        // A ring 3% of the size wide holds 1.2 mm from 40 mm up.
        assert!((30.0..=50.0).contains(&b), "ring needs {b} mm");
    }
}
