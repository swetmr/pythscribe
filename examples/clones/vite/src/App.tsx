import { Route, Routes } from 'react-router'
import Home from './pages/Home'
import Hello from './pages/Hello'
import HelloReference from './pages/HelloReference'
import YouTube from './pages/clones/YouTube'
import YouTubeReference from './pages/clones/YouTubeReference'
import YouTubePsc from './pages/clones/YouTubePsc'
import Netflix from './pages/clones/Netflix'
import NetflixReference from './pages/clones/NetflixReference'
import Coursera from './pages/clones/Coursera'
import CourseraReference from './pages/clones/CourseraReference'
import CourseraPsc from './pages/clones/CourseraPsc'
import Spotify from './pages/clones/Spotify'
import SpotifyReference from './pages/clones/SpotifyReference'
import Kanban from './pages/clones/Kanban'
import KanbanReference from './pages/clones/KanbanReference'
import Game2048 from './pages/clones/Game2048'
import Game2048Reference from './pages/clones/Game2048Reference'
import Game2048Psc from './pages/clones/Game2048Psc'
import Tetris from './pages/clones/Tetris'
import TetrisReference from './pages/clones/TetrisReference'
import TetrisPsc from './pages/clones/TetrisPsc'
import Twitter from './pages/clones/Twitter'
import TwitterReference from './pages/clones/TwitterReference'
import NetflixPsc from './pages/clones/NetflixPsc'
import SpotifyPsc from './pages/clones/SpotifyPsc'
import KanbanPsc from './pages/clones/KanbanPsc'
import TwitterPsc from './pages/clones/TwitterPsc'
import HelloPsc from './pages/HelloPsc'

// Routes are PRE-WIRED for every clone. Clone agents replace the body of
// their pages/clones/<Name>.tsx files and never edit this file.
export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
      <Route path="/hello" element={<Hello />} />
      <Route path="/react-reference/hello" element={<HelloReference />} />
      <Route path="/youtube" element={<YouTube />} />
      <Route path="/netflix" element={<Netflix />} />
      <Route path="/coursera" element={<Coursera />} />
      <Route path="/spotify" element={<Spotify />} />
      <Route path="/kanban" element={<Kanban />} />
      <Route path="/2048" element={<Game2048 />} />
      <Route path="/tetris" element={<Tetris />} />
      <Route path="/twitter" element={<Twitter />} />
      <Route path="/youtube-psc" element={<YouTubePsc />} />
      <Route path="/coursera-psc" element={<CourseraPsc />} />
      <Route path="/netflix-psc" element={<NetflixPsc />} />
      <Route path="/spotify-psc" element={<SpotifyPsc />} />
      <Route path="/kanban-psc" element={<KanbanPsc />} />
      <Route path="/2048-psc" element={<Game2048Psc />} />
      <Route path="/tetris-psc" element={<TetrisPsc />} />
      <Route path="/twitter-psc" element={<TwitterPsc />} />
      <Route path="/hello-psc" element={<HelloPsc />} />
      <Route path="/react-reference/youtube" element={<YouTubeReference />} />
      <Route path="/react-reference/netflix" element={<NetflixReference />} />
      <Route path="/react-reference/coursera" element={<CourseraReference />} />
      <Route path="/react-reference/spotify" element={<SpotifyReference />} />
      <Route path="/react-reference/kanban" element={<KanbanReference />} />
      <Route path="/react-reference/2048" element={<Game2048Reference />} />
      <Route path="/react-reference/tetris" element={<TetrisReference />} />
      <Route path="/react-reference/twitter" element={<TwitterReference />} />
    </Routes>
  )
}
