<!-- exp=macrov2-haiku task=macro_playlist_player cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.061592400000000005 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": 1,
        "name": "Summer Vibes",
        "tracks": [
            {"id": 101, "title": "Electric Dreams", "artist": "Neon Lights", "duration": "3:42"},
            {"id": 102, "title": "Sunset Boulevard", "artist": "Golden Hour", "duration": "4:15"},
            {"id": 103, "title": "Neon Nights", "artist": "Synth Wave", "duration": "3:58"},
            {"id": 104, "title": "City Lights", "artist": "Urban Beats", "duration": "4:32"},
        ]
    },
    {
        "id": 2,
        "name": "Focus Work",
        "tracks": [
            {"id": 201, "title": "Deep Focus", "artist": "Ambient Mind", "duration": "5:12"},
            {"id": 202, "title": "Concentration Zone", "artist": "Lo-Fi Beats", "duration": "4:45"},
            {"id": 203, "title": "Productive Flow", "artist": "Chill Waves", "duration": "6:20"},
        ]
    },
    {
        "id": 3,
        "name": "Workout Mix",
        "tracks": [
            {"id": 301, "title": "Pump It Up", "artist": "High Energy", "duration": "3:30"},
            {"id": 302, "title": "Maximum Power", "artist": "Beat Droppers", "duration": "4:05"},
            {"id": 303, "title": "Cardio Rush", "artist": "Tempo Beats", "duration": "3:15"},
            {"id": 304, "title": "Thunder Down", "artist": "Bass Boosters", "duration": "3:48"},
            {"id": 305, "title": "Final Sprint", "artist": "Peak Performance", "duration": "4:22"},
        ]
    },
]

@component
def PlaylistPlayer():
    active_playlist_id, set_active_playlist_id = use_state(1)
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)
    
    active_playlist = next(p for p in PLAYLISTS if p["id"] == active_playlist_id)
    now_playing_track = next(
        (t for t in active_playlist["tracks"] if t["id"] == now_playing_id),
        None
    )
    
    def switch_playlist(playlist_id):
        set_active_playlist_id(playlist_id)
        set_now_playing_id(None)
        set_is_playing(False)
    
    def select_track(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)
    
    def toggle_play():
        set_is_playing(not is_playing)
    
    return div(class_name="playlist-player",
        div(class_name="sidebar",
            div(class_name="sidebar-header", "Playlists"),
            div(class_name="playlists-list",
                *[div(class_name=f"playlist-item{' active' if p['id'] == active_playlist_id else ''}",
                      on_click=lambda: switch_playlist(p["id"]),
                      div(class_name="playlist-name", p["name"]),
                      div(class_name="playlist-count", f"{len(p['tracks'])} tracks"))
                  for p in PLAYLISTS]
            )
        ),
        div(class_name="main-panel",
            div(class_name="playlist-header", active_playlist["name"]),
            div(class_name="tracks-list",
                *[div(class_name=f"track-item{' now-playing' if t['id'] == now_playing_id else ''}",
                      on_click=lambda: select_track(t["id"]),
                      div(class_name="track-info",
                          div(class_name="track-title", t["title"]),
                          div(class_name="track-artist", t["artist"])),
                      div(class_name="track-duration", t["duration"]))
                  for t in active_playlist["tracks"]]
            )
        ),
        div(class_name="now-playing-bar",
            div(class_name="now-playing-info",
                now_playing_track["title"] if now_playing_track else "No track selected"),
            button(class_name="play-pause-btn",
                   on_click=lambda: toggle_play(),
                   "▶ Play" if not is_playing else "⏸ Pause")
        )
    )
```
