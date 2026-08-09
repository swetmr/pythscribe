<!-- exp=macrov2-haiku task=macro_playlist_player cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0356119 -->
```python
"use client"
R*

@c
def PlaylistPlayer():
    playlists = [
        {"id": 1, "name": "Summer Vibes", "tracks": [
            {"id": 1, "title": "Sunshine", "artist": "The Warm Tones", "duration": "3:45"},
            {"id": 2, "title": "Beach Day", "artist": "Ocean Waves", "duration": "4:12"},
            {"id": 3, "title": "Golden Hour", "artist": "Sunset Collective", "duration": "3:28"},
        ]},
        {"id": 2, "name": "Focus Mode", "tracks": [
            {"id": 4, "title": "Deep Work", "artist": "Concentration Lab", "duration": "5:10"},
            {"id": 5, "title": "Flow State", "artist": "Productivity Beats", "duration": "4:55"},
            {"id": 6, "title": "Mindful Coding", "artist": "Zen Engineers", "duration": "3:30"},
        ]},
        {"id": 3, "name": "Night Drive", "tracks": [
            {"id": 7, "title": "Neon Lights", "artist": "Synth Masters", "duration": "4:02"},
            {"id": 8, "title": "City Streets", "artist": "Night Riders", "duration": "3:47"},
            {"id": 9, "title": "Echo and Fade", "artist": "Dream Walkers", "duration": "4:28"},
        ]},
    ]
    
    active_id, set_active = us(1)
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)
    
    active_playlist = next((p for p in playlists if p["id"] == active_id), playlists[0])
    now_track = next((t for t in active_playlist["tracks"] if t["id"] == now_playing), None)
    
    def make_select_playlist(pid):
        return lambda: set_active(pid)
    
    def make_select_track(tid):
        return lambda: set_now_playing(tid)
    
    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            ul(*[
                li(cn="playlist-item" + (" active" if p["id"] == active_id else ""),
                   oc=make_select_playlist(p["id"]),
                   div(cn="playlist-name", p["name"]),
                   div(cn="track-count", f"{len(p['tracks'])} tracks"),
                )
                for p in playlists
            ]),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(*[
                li(cn="track-item" + (" now-playing" if t["id"] == now_playing else ""),
                   oc=make_select_track(t["id"]),
                   div(cn="track-title", t["title"]),
                   div(cn="track-artist", t["artist"]),
                   div(cn="track-duration", t["duration"]),
                )
                for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="player-bar",
            div(cn="now-playing-info",
                "Now Playing: " + (now_track["title"] if now_track else "Nothing selected"),
            ),
            button(oc=lambda: set_is_playing(not is_playing),
                   cn="play-button",
                   "Play" if not is_playing else "Pause",
            ),
        ),
    )
```
