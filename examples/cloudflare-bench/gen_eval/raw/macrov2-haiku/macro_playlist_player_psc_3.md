<!-- exp=macrov2-haiku task=macro_playlist_player cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0310119 -->
```python
"use client"
R*

@c
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Summer Hits",
            "tracks": [
                {"id": 1, "title": "Sunshine", "artist": "The Rays", "duration": "3:45"},
                {"id": 2, "title": "Beach Day", "artist": "Ocean Waves", "duration": "3:12"},
                {"id": 3, "title": "Golden Hour", "artist": "Sunset Band", "duration": "4:02"},
            ]
        },
        {
            "id": 2,
            "name": "Chill Vibes",
            "tracks": [
                {"id": 4, "title": "Soft Rain", "artist": "Ambient Studio", "duration": "5:30"},
                {"id": 5, "title": "Moonlight", "artist": "Night Owl", "duration": "4:15"},
                {"id": 6, "title": "Peaceful Mind", "artist": "Zen Master", "duration": "6:00"},
            ]
        },
        {
            "id": 3,
            "name": "Workout Energy",
            "tracks": [
                {"id": 7, "title": "Pump It Up", "artist": "Fit Crew", "duration": "3:30"},
                {"id": 8, "title": "Go Harder", "artist": "Power Rush", "duration": "3:45"},
                {"id": 9, "title": "Victory", "artist": "Champions", "duration": "3:20"},
            ]
        },
    ]

    active_id, set_active_id = us(playlists[0]["id"])
    playing_id, set_playing_id = us(None)
    is_playing, set_is_playing = us(False)

    active_pl = next(p for p in playlists if p["id"] == active_id)
    playing_track = next((t for t in active_pl["tracks"] if t["id"] == playing_id), None)

    def select_playlist(pid):
        set_active_id(pid)
        set_playing_id(None)
        set_is_playing(False)

    def play_track(tid):
        set_playing_id(tid)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            div(cn="playlist-list",
                *[div(cn=f"playlist-item {'active' if p['id'] == active_id else ''}",
                      oc=lambda: select_playlist(p["id"]),
                      div(cn="playlist-name", p["name"]),
                      div(cn="track-count", f"{len(p['tracks'])} tracks"))
                  for p in playlists]
            ),
        ),
        div(cn="main-content",
            h2(active_pl["name"]),
            div(cn="track-list",
                *[div(cn=f"track-item {'now-playing' if t['id'] == playing_id else ''}",
                      oc=lambda: play_track(t["id"]),
                      div(cn="track-info",
                          div(cn="track-title", t["title"]),
                          div(cn="track-artist", t["artist"]),
                      ),
                      div(cn="track-duration", t["duration"]))
                  for t in active_pl["tracks"]]
            ),
        ),
        div(cn="player-bar",
            div(cn="now-playing-info",
                div(cn="now-playing-label", "Now Playing:"),
                div(cn="now-playing-title",
                    playing_track["title"] if playing_track else "No track selected"),
            ),
            button(oc=lambda: toggle_play(),
                   cn="play-button",
                   "⏸ Pause" if is_playing else "▶ Play"),
        ),
    )
```
