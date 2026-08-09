<!-- exp=baseline-haiku task=macro_playlist_player cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0358202 -->
```python
"use client"

from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Summer Vibes",
            "tracks": [
                {"id": 1, "title": "Sunny Days", "artist": "The Bright Side", "duration": "3:45"},
                {"id": 2, "title": "Golden Hour", "artist": "Luna Wave", "duration": "4:12"},
                {"id": 3, "title": "Beach Life", "artist": "Coastal Beats", "duration": "3:28"},
            ]
        },
        {
            "id": 2,
            "name": "Chill Beats",
            "tracks": [
                {"id": 4, "title": "Ambient Dreams", "artist": "Peaceful Mind", "duration": "5:33"},
                {"id": 5, "title": "Midnight Rain", "artist": "Soft Echo", "duration": "4:47"},
                {"id": 6, "title": "Silent Night", "artist": "Calm Waters", "duration": "3:52"},
            ]
        },
        {
            "id": 3,
            "name": "Rock Classics",
            "tracks": [
                {"id": 7, "title": "Thunder Road", "artist": "Stone Valley", "duration": "4:15"},
                {"id": 8, "title": "Electric Dreams", "artist": "Neon Lights", "duration": "3:59"},
                {"id": 9, "title": "Breaking Free", "artist": "The Rebels", "duration": "4:33"},
            ]
        },
    ]

    selected_playlist_id, set_selected_playlist_id = use_state(1)
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)

    selected_playlist = next((p for p in playlists if p["id"] == selected_playlist_id), playlists[0])
    now_playing_track = next((t for t in selected_playlist["tracks"] if t["id"] == now_playing_id), None)

    def select_playlist(playlist_id):
        set_selected_playlist_id(playlist_id)
        set_now_playing_id(None)
        set_is_playing(False)

    def play_track(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(class_name="player-container",
        div(class_name="sidebar",
            h3("Playlists"),
            ul(class_name="playlist-list",
                *[li(key=p["id"],
                     class_name=f"playlist-item {'active' if p['id'] == selected_playlist_id else ''}",
                     on_click=lambda pid=p["id"]: select_playlist(pid),
                     div(class_name="playlist-name", p["name"]),
                     div(class_name="track-count", f"{len(p['tracks'])} tracks"))
                  for p in playlists]
            )
        ),
        div(class_name="main-content",
            div(class_name="header", h2(selected_playlist["name"])),
            div(class_name="tracks-container",
                ul(class_name="track-list",
                    *[li(key=t["id"],
                         class_name=f"track-item {'playing' if t['id'] == now_playing_id else ''}",
                         on_click=lambda tid=t["id"]: play_track(tid),
                         div(class_name="track-info",
                             div(class_name="track-title", t["title"]),
                             div(class_name="track-artist", t["artist"])
                         ),
                         div(class_name="track-duration", t["duration"]))
                      for t in selected_playlist["tracks"]]
                )
            )
        ),
        div(class_name="player-bottom",
            div(class_name="now-playing-info",
                p(class_name="now-playing-title",
                  now_playing_track["title"] if now_playing_track else "No track selected")
            ),
            button(class_name="play-button",
                   on_click=lambda: toggle_play(),
                   "▶ Play" if not is_playing else "⏸ Pause")
        )
    )
```
