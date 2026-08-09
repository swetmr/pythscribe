<!-- exp=macrov2-haiku task=macro_playlist_player cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.04065740000000001 -->
```python
"use client"

from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Chill Vibes",
            "tracks": [
                {"id": "chill-1", "title": "Sunset", "artist": "Luna", "duration": "3:45"},
                {"id": "chill-2", "title": "Ocean Breeze", "artist": "Aqua Dreams", "duration": "4:12"},
                {"id": "chill-3", "title": "Peaceful", "artist": "Zen Master", "duration": "5:30"},
            ]
        },
        {
            "id": 2,
            "name": "Road Trip",
            "tracks": [
                {"id": "road-1", "title": "Highway", "artist": "The Wanderers", "duration": "3:20"},
                {"id": "road-2", "title": "Mile Marker", "artist": "Journey", "duration": "4:05"},
                {"id": "road-3", "title": "Destination", "artist": "The Road", "duration": "3:55"},
            ]
        },
        {
            "id": 3,
            "name": "Workout Mix",
            "tracks": [
                {"id": "work-1", "title": "Pump It Up", "artist": "Energy Burst", "duration": "3:10"},
                {"id": "work-2", "title": "Running Wild", "artist": "Fast Track", "duration": "3:40"},
                {"id": "work-3", "title": "Maximum Power", "artist": "Strength", "duration": "4:00"},
            ]
        },
    ]
    
    active_playlist_id, set_active_playlist_id = use_state(1)
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)
    
    active_playlist = next((p for p in playlists if p["id"] == active_playlist_id), playlists[0])
    
    def select_playlist(pid):
        set_active_playlist_id(pid)
        set_now_playing_id(None)
        set_is_playing(False)
    
    def play_track(track):
        set_now_playing_id(track["id"])
        set_is_playing(True)
    
    def toggle_play():
        set_is_playing(not is_playing)
    
    now_playing_track = None
    if now_playing_id:
        for playlist in playlists:
            for track in playlist["tracks"]:
                if track["id"] == now_playing_id:
                    now_playing_track = track
                    break
    
    return div(class_name="playlist-player",
        div(class_name="player-content",
            div(class_name="sidebar",
                h2("Playlists"),
                div(class_name="playlist-list",
                    *[div(
                        class_name=f"playlist-item {'active' if p['id'] == active_playlist_id else ''}",
                        on_click=lambda: select_playlist(p["id"]),
                        div(class_name="playlist-name", p["name"]),
                        div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in playlists]
                )
            ),
            div(class_name="main-panel",
                h2(active_playlist["name"]),
                div(class_name="track-list",
                    *[div(
                        class_name=f"track-item {'now-playing' if now_playing_id == track['id'] else ''}",
                        on_click=lambda: play_track(track),
                        div(class_name="track-info",
                            div(class_name="track-title", track["title"]),
                            div(class_name="track-artist", track["artist"]),
                        ),
                        div(class_name="track-duration", track["duration"]),
                    ) for track in active_playlist["tracks"]]
                )
            ),
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                p(now_playing_track["title"]) if now_playing_track else p("Select a track"),
            ),
            button(
                class_name="play-pause-btn",
                on_click=lambda: toggle_play(),
                "⏸" if is_playing else "▶",
            ),
        )
    )
```
