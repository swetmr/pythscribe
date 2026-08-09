// Local mock data ONLY — no network calls from shared/ (see CONTRIBUTING.md).
// Netflix clone catalog: 4 content rows (>=10 titles each) + a hero title.
// Several titles deliberately appear in MORE THAN ONE row (t01, t02, t03,
// t05) so the My List toggle can be asserted to stay in sync across
// carousels — the core context stress-test of this clone.
//
// Card/hero/modal art renders a local placeholder poster image (cover) when a
// title has `poster` set, falling back to the CSS gradient pair otherwise.

export interface NetflixTitle {
  id: string
  name: string
  description: string
  year: number
  maturity: string
  duration: string
  match: number
  genres: string[]
  art: [string, string]
  // Optional poster image (local placeholder). When set, the card/hero/modal
  // art renders it as a cover background-image; the `art` gradient stays as a
  // color fallback. Populated deterministically below (poster{k%12}.jpg).
  poster?: string
}

export interface NetflixRow {
  id: string
  label: string
  title_ids: string[]
}

const t = (
  id: string,
  name: string,
  description: string,
  year: number,
  maturity: string,
  duration: string,
  match: number,
  genres: string[],
  art: [string, string],
): NetflixTitle => ({ id, name, description, year, maturity, duration, match, genres, art })

// Real film TITLES + YEARS are facts (fine to use). Descriptions below are
// ORIGINAL, generic one-liners written for this demo — NOT copied from any
// real synopsis or tagline. maturity/duration/match/art are neutral
// placeholder metadata (not claimed as accurate). Ids/rows are unchanged so
// the cross-row My List sync tests keep working.
const all: NetflixTitle[] = [
  // Trending Now (t01–t12)
  t('t01', 'The Dark Knight', 'A masked vigilante and a chaos-loving anarchist collide over one city\'s fate.', 2008, 'TV-MA', '1h 58m', 97, ['Action', 'Crime'], ['#3d1c56', '#0f4c81']),
  t('t02', 'Inception', 'A thief who steals secrets from dreams takes on one last impossible job.', 2010, 'PG-13', '2h 04m', 93, ['Sci-Fi', 'Thriller'], ['#7a3b2e', '#c98a2b']),
  t('t03', 'Forrest Gump', 'An ordinary man drifts through decades of history and one enduring love.', 1994, 'TV-14', '1h 37m', 88, ['Drama', 'Romance'], ['#0b6e4f', '#f2c14e']),
  t('t04', 'Pulp Fiction', 'Overlapping tales of hitmen, a boxer, and a very bad day, told out of order.', 1994, 'TV-MA', '1h 49m', 91, ['Crime', 'Drama'], ['#1b1b2f', '#e43f5a']),
  t('t05', 'The Matrix', 'A hacker learns his reality is a simulation and joins the fight to break it.', 1999, 'PG-13', '2h 11m', 95, ['Sci-Fi', 'Action'], ['#0d1b2a', '#41729f']),
  t('t06', 'Fight Club', 'An insomniac office worker starts an underground club that spirals out of control.', 1999, 'TV-PG', '1h 44m', 86, ['Drama', 'Thriller'], ['#5c2018', '#e07a5f']),
  t('t07', 'The Godfather', 'A reluctant heir is pulled into the family business of crime and loyalty.', 1972, 'TV-MA', '1h 52m', 90, ['Crime', 'Drama'], ['#2d132c', '#801336']),
  t('t08', 'Gladiator', 'A betrayed general fights his way up from the arena to reclaim his honor.', 2000, 'TV-Y7', '1h 29m', 84, ['Action', 'Drama'], ['#1a936f', '#88d498']),
  t('t09', 'The Departed', 'A cop and a criminal each go undercover and race to unmask the other.', 2006, 'TV-MA', '2h 02m', 92, ['Crime', 'Thriller'], ['#3e424b', '#b09b71']),
  t('t10', 'Interstellar', 'A pilot leaves Earth through a wormhole to find humanity a new home.', 2014, 'PG-13', '1h 41m', 87, ['Sci-Fi', 'Drama'], ['#232b5c', '#d95d39']),
  t('t11', 'Joker', 'A failing comedian\'s slow unraveling turns him into a symbol of unrest.', 2019, 'TV-14', '1h 56m', 89, ['Crime', 'Drama'], ['#a2d5f2', '#07689f']),
  t('t12', 'Django Unchained', 'A freed gunslinger crosses a brutal frontier to rescue the woman he loves.', 2012, 'TV-MA', '1h 47m', 94, ['Western', 'Drama'], ['#4a0e4e', '#a4508b']),
  // Critically Acclaimed (r01–r10)
  t('r01', 'The Shawshank Redemption', 'A wrongly convicted banker holds onto hope across long years behind bars.', 1994, 'PG', '1h 51m', 96, ['Drama'], ['#606c38', '#fefae0']),
  t('r02', 'Schindler\'s List', 'A wartime businessman risks everything to shelter workers from disaster.', 1993, 'PG-13', '2h 08m', 95, ['Drama', 'History'], ['#283618', '#bc6c25']),
  t('r03', '12 Angry Men', 'One juror stands alone against eleven, forcing a room to think twice.', 1957, 'TV-MA', '1h 59m', 94, ['Drama', 'Crime'], ['#14213d', '#fca311']),
  t('r04', 'Goodfellas', 'A young man\'s rise and fall inside a tight-knit criminal crew.', 1990, 'PG', '2h 21m', 97, ['Crime', 'Drama'], ['#457b9d', '#f1faee']),
  t('r05', 'Se7en', 'Two detectives chase a killer who stages crimes around the seven deadly sins.', 1995, 'TV-14', '1h 43m', 93, ['Crime', 'Thriller'], ['#6d597a', '#e56b6f']),
  t('r06', 'The Silence of the Lambs', 'A young agent bargains with an imprisoned genius to catch a killer.', 1991, 'PG', '2h 15m', 92, ['Crime', 'Thriller'], ['#355070', '#eaac8b']),
  t('r07', 'Saving Private Ryan', 'A squad pushes across enemy lines to bring one soldier safely home.', 1998, 'TV-PG', '1h 38m', 91, ['Drama', 'War'], ['#f4d35e', '#0d3b66']),
  t('r08', 'The Green Mile', 'A death-row guard meets a gentle prisoner with an inexplicable gift.', 1999, 'TV-14', '1h 54m', 95, ['Drama', 'Fantasy'], ['#1d3557', '#e63946']),
  t('r09', 'Whiplash', 'A driven drummer and a merciless mentor push each other past the breaking point.', 2014, 'TV-MA', '2h 06m', 96, ['Drama', 'Music'], ['#2b2d42', '#ef233c']),
  t('r10', 'The Prestige', 'Two rival magicians trade escalating secrets in a deadly game of one-upmanship.', 2006, 'PG', '1h 33m', 90, ['Drama', 'Mystery'], ['#8ecae6', '#fb8500']),
  // Sci-Fi Epics (s01–s10)
  t('s01', 'Blade Runner 2049', 'A synthetic detective uncovers a buried secret that could upend his world.', 2017, 'TV-14', '2h 22m', 94, ['Sci-Fi', 'Drama'], ['#ff7b00', '#3c096c']),
  t('s02', 'Alien', 'A deep-space crew answers a distress call and brings something deadly aboard.', 1979, 'PG-13', '2h 09m', 91, ['Sci-Fi', 'Horror'], ['#03045e', '#00b4d8']),
  t('s03', 'WALL·E', 'A lonely cleanup robot on an abandoned Earth follows a spark of hope skyward.', 2008, 'TV-MA', '1h 57m', 93, ['Sci-Fi', 'Animation'], ['#000000', '#495057']),
  t('s04', 'Terminator 2: Judgment Day', 'A machine sent back in time becomes the last line of defense for a boy.', 1991, 'TV-14', '1h 48m', 89, ['Sci-Fi', 'Action'], ['#9d0208', '#ffba08']),
  t('s05', 'Back to the Future', 'A teenager strands himself in the past and races to fix the timeline home.', 1985, 'PG-13', '2h 01m', 92, ['Sci-Fi', 'Adventure'], ['#10002b', '#c77dff']),
  t('s06', 'Jurassic Park', 'A theme park of revived dinosaurs goes catastrophically off script.', 1993, 'TV-MA', '2h 14m', 95, ['Sci-Fi', 'Adventure'], ['#001219', '#94d2bd']),
  t('s07', 'A Beautiful Mind', 'A brilliant mathematician wrestles with genius and the limits of his own mind.', 2001, 'TV-PG', '1h 35m', 87, ['Drama', 'Biography'], ['#3a0ca3', '#f72585']),
  t('s08', '1917', 'Two soldiers race against the clock to deliver a life-or-death message.', 2019, 'TV-14', '1h 53m', 90, ['War', 'Drama'], ['#590d22', '#ff8fab']),
  t('s09', 'No Country for Old Men', 'A hunter takes a bag of cash and a relentless killer follows the trail.', 2007, 'TV-PG', '1h 46m', 88, ['Crime', 'Thriller'], ['#2d6a4f', '#d8f3dc']),
  t('s10', 'Parasite', 'One struggling family schemes its way into the household of another.', 2019, 'TV-14', '2h 18m', 96, ['Thriller', 'Drama'], ['#161a1d', '#a4161a']),
  // Comedies (c01–c10)
  t('c01', 'Toy Story', 'When a shiny new toy arrives, a favorite cowboy fights to stay loved.', 1995, 'TV-PG', '1h 31m', 85, ['Animation', 'Family'], ['#f77f00', '#003049']),
  t('c02', 'The Lion King', 'A young prince flees his home and must return to claim who he is.', 1994, 'TV-14', '1h 44m', 83, ['Animation', 'Family'], ['#ffafcc', '#390099']),
  t('c03', 'Up', 'A widower ties his house to balloons and drifts toward a long-promised adventure.', 2009, 'TV-14', '1h 36m', 89, ['Animation', 'Adventure'], ['#0077b6', '#caf0f8']),
  t('c04', 'Coco', 'A boy chasing music slips into a dazzling land to uncover his family\'s past.', 2017, 'TV-MA', '1h 40m', 86, ['Animation', 'Family'], ['#6a040f', '#ffd166']),
  t('c05', 'Spirited Away', 'A girl trapped in a spirit world bargains for her parents\' freedom.', 2001, 'PG-13', '1h 39m', 84, ['Animation', 'Fantasy'], ['#7209b7', '#4cc9f0']),
  t('c06', 'The Truman Show', 'A man discovers his entire pleasant life is a broadcast built around him.', 1998, 'TV-PG', '1h 42m', 87, ['Comedy', 'Drama'], ['#38b000', '#9ef01a']),
  t('c07', 'Amadeus', 'A court composer is consumed by envy of a rival\'s effortless genius.', 1984, 'TV-14', '1h 34m', 82, ['Drama', 'Music'], ['#bc8034', '#ffe8c2']),
  t('c08', 'Braveheart', 'A farmer turned rebel rallies his countrymen against a distant crown.', 1995, 'TV-PG', '1h 28m', 85, ['Action', 'History'], ['#d00000', '#ffba08']),
  t('c09', 'Casablanca', 'In a wartime city, an old flame reopens a choice a jaded man thought was closed.', 1942, 'TV-14', '1h 32m', 88, ['Drama', 'Romance'], ['#4f772d', '#ecf39e']),
  t('c10', 'Rear Window', 'A housebound photographer becomes convinced a neighbor has done something terrible.', 1954, 'TV-MA', '1h 45m', 86, ['Mystery', 'Thriller'], ['#1b4965', '#bee9e8']),
]

// Attach a deterministic local placeholder poster to every title (12 images,
// cycled by array index). The `art` gradient stays as a color fallback.
export const netflixTitles: Record<string, NetflixTitle> = Object.fromEntries(
  all.map((x, k) => [x.id, { ...x, poster: `/media/posters/poster${k % 12}.jpg` }]),
)

// t01/t05 recur in Critically Acclaimed, t02 in Sci-Fi Epics, t03 in
// Comedies — the cross-row My List sync cases.
export const netflixRows: NetflixRow[] = [
  { id: 'trending', label: 'Trending Now', title_ids: ['t01', 't02', 't03', 't04', 't05', 't06', 't07', 't08', 't09', 't10', 't11', 't12'] },
  { id: 'acclaimed', label: 'Critically Acclaimed', title_ids: ['r01', 'r02', 't01', 'r03', 'r04', 'r05', 't05', 'r06', 'r07', 'r08', 'r09', 'r10'] },
  { id: 'scifi', label: 'Sci-Fi Epics', title_ids: ['s01', 's02', 's03', 't02', 's04', 's05', 's06', 's07', 's08', 's09', 's10'] },
  { id: 'comedies', label: 'Comedies', title_ids: ['c01', 'c02', 't03', 'c03', 'c04', 'c05', 'c06', 'c07', 'c08', 'c09', 'c10'] },
]

export const netflixHeroId = 't01'

export const netflixFixture = {
  titles: netflixTitles,
  rows: netflixRows,
  hero_id: netflixHeroId,
}
