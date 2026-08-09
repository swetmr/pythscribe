<!-- exp=baseline-haiku task=macro_playlist_player cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.048514400000000006 -->
```python
"use client"
R*

playlists = [
    {"name": "Chill Vibes", "tracks": [
        {"id": 0, "title": "Lazy Day", "artist": "Sky Walker", "duration": "3:45"},
        {"id": 1, "title": "Ocean Waves", "artist": "Blue Hour", "duration": "4:12"},
        {"id": 2, "title": "Sunset Dream", "artist": "Warm Light", "duration": "3:30"},
    ]},
    {"name": "Workout Mix", "tracks": [
        {"id": 3, "title": "Energy Boost", "artist": "Beat Pusher", "duration": "3:15"},
        {"id": 4, "title": "Running Strong", "artist": "Pulse Makers", "duration": "3:50"},
        {"id": 5, "title": "Go Hard", "artist": "Thunder Road", "duration": "3:40"},
    ]},
    {"name": "Study Beats", "tracks": [
        {"id": 6, "title": "Focus Flow", "artist": "Calm Mind", "duration": "5:20"},
        {"id": 7, "title": "Concentration", "artist": "Silent Voice", "duration": "4:45"},
        {"id": 8, "title": "Deep Work", "artist": "Zen Master", "duration": "6:00"},
    ]},
]

@c
def PlaylistPlayer():
    active_idx, set_active_idx = us(0)
    now_playing_id, set_now_playing_id = us(None)
    is_playing, set_is_playing = us(False)
    
    playlist = playlists[active_idx]
    tracks = playlist["tracks"]
    now_playing = next((t for t in tracks if t["id"] == now_playing_id), None)
    
    return div(cn="player",
        div(cn="sidebar",
            h2("Playlists"),
            div(cn="playlist-list",
                *[div(key=i, cn="playlist-item" + (" active" if i == active_idx else ""),
                      oc=lambda idx=i: set_active_idx(idx),
                      div(cn="playlist-name", playlists[i]["name"]),
                      div(cn="playlist-count", f"{len(playlists[i]['tracks'])} tracks"))
                  for i in range(len(playlists))]),
        ),
        div(cn="main-content",
            div(cn="playlist-header", h1(playlist["name"])),
            div(cn="tracks-list",
                *[div(key=t["id"], cn="track-item" + (" playing" if t["id"] == now_playing_id else ""),
                      oc=lambda track_id=t["id"]: set_now_playing_id(track_id),
                      div(cn="track-title", t["title"]),
                      div(cn="track-artist", t["artist"]),
                      div(cn="track-duration", t["duration"]))
                  for t in tracks]),
            ),
        ),
        div(cn="bottom-player",
            div(cn="now-playing-info",
                div(cn="now-playing-text",
                    f"{now_playing['title']} - {now_playing['artist']}")
                if now_playing else div(cn="no-selection", "Select a track"),
            ),
            button(cn="play-button", oc=lambda: set_is_playing(not is_playing),
                   "▶" if not is_playing else "⏸"),
        ),
    )
```
