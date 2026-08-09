<!-- exp=macrov2-haiku task=macro_playlist_player cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0319224 -->
```python
"use client"

from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists = [
        {"id": 1, "name": "Summer Vibes", "tracks": [
            {"id": 1, "title": "Sunset Dreamer", "artist": "Luna Sky", "duration": "3:24"},
            {"id": 2, "title": "Ocean Waves", "artist": "The Tides", "duration": "2:58"},
            {"id": 3, "title": "Golden Hour", "artist": "Amber Dreams", "duration": "3:45"},
        ]},
        {"id": 2, "name": "Workout Mix", "tracks": [
            {"id": 4, "title": "Pump It Up", "artist": "Energy Rush", "duration": "4:12"},
            {"id": 5, "title": "Beat Drop", "artist": "Bass Kings", "duration": "3:33"},
            {"id": 6, "title": "Run Fast", "artist": "Speed Demons", "duration": "3:07"},
        ]},
        {"id": 3, "name": "Chill Lounge", "tracks": [
            {"id": 7, "title": "Rainy Day", "artist": "Soft Voices", "duration": "4:02"},
            {"id": 8, "title": "Coffee Shop", "artist": "Ambient Minds", "duration": "3:50"},
            {"id": 9, "title": "Midnight Jazz", "artist": "Smooth Cats", "duration": "4:28"},
        ]},
    ]
    
    active_playlist_id, set_active_playlist_id = use_state(1)
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)
    
    active_playlist = next((p for p in playlists if p["id"] == active_playlist_id), playlists[0])
    now_playing_track = next((t for t in active_playlist["tracks"] if t["id"] == now_playing_id), None)
    
    def select_playlist(playlist_id):
        set_active_playlist_id(playlist_id)
    
    def select_track(track_id):
        set_now_playing_id(track_id)
    
    def toggle_play():
        set_is_playing(not is_playing)
    
    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h3("Playlists"),
            *[div(
                class_name=f"playlist-item {'active' if p['id'] == active_playlist_id else ''}",
                on_click=lambda: select_playlist(p["id"]),
                div(class_name="playlist-name", p["name"]),
                div(class_name="track-count", f"{len(p['tracks'])} tracks"),
            ) for p in playlists]
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"]),
            div(class_name="tracks-list",
                *[div(
                    class_name=f"track-item {'now-playing' if t['id'] == now_playing_id else ''}",
                    on_click=lambda: select_track(t["id"]),
                    div(class_name="track-title", t["title"]),
                    div(class_name="track-artist", t["artist"]),
                    div(class_name="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]]
            ),
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                now_playing_track["title"] if now_playing_track else "No track selected",
            ),
            button(
                class_name="play-pause-btn",
                on_click=lambda: toggle_play(),
                "⏸" if is_playing else "▶",
            ),
        ),
    )
```
