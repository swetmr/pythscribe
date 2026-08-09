<!-- exp=baseline-haiku task=macro_playlist_player cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.041313300000000004 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "playlist-1",
        "name": "Summer Hits",
        "tracks": [
            {"id": "track-1", "title": "Sunny Days", "artist": "The Bright Band", "duration": "3:45"},
            {"id": "track-2", "title": "Beach Vibes", "artist": "Ocean Waves", "duration": "4:12"},
            {"id": "track-3", "title": "Golden Hour", "artist": "Sunset Collective", "duration": "3:58"},
            {"id": "track-4", "title": "Tropical Paradise", "artist": "Island Beats", "duration": "3:30"},
        ]
    },
    {
        "id": "playlist-2",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "track-5", "title": "Midnight Rain", "artist": "Quiet Storm", "duration": "4:23"},
            {"id": "track-6", "title": "Soft Echo", "artist": "Ambient Journey", "duration": "5:10"},
            {"id": "track-7", "title": "Peaceful Mind", "artist": "Zen Music", "duration": "4:45"},
        ]
    },
    {
        "id": "playlist-3",
        "name": "Rock Classics",
        "tracks": [
            {"id": "track-8", "title": "Electric Thunder", "artist": "The Rockers", "duration": "3:55"},
            {"id": "track-9", "title": "Stone Wall", "artist": "Classic Rock Band", "duration": "4:32"},
            {"id": "track-10", "title": "Guitar Storm", "artist": "Heavy Metal Kings", "duration": "3:20"},
            {"id": "track-11", "title": "Amplified Dreams", "artist": "Power Chords", "duration": "4:08"},
            {"id": "track-12", "title": "Midnight Rocker", "artist": "The Rockers", "duration": "3:42"},
        ]
    },
]

@component
def PlaylistPlayer():
    active_playlist_id, set_active_playlist_id = use_state("playlist-1")
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active_playlist = next(
        (p for p in PLAYLISTS if p["id"] == active_playlist_id),
        PLAYLISTS[0]
    )

    now_playing_track = next(
        (t for t in active_playlist["tracks"] if t["id"] == now_playing_id),
        None
    )

    def handle_track_click(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)

    def toggle_playback():
        set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h3("Playlists"),
            ul(*[
                li(
                    key=p["id"],
                    class_name=f"playlist-item {'active' if p['id'] == active_playlist_id else ''}",
                    on_click=lambda pid=p["id"]: set_active_playlist_id(pid),
                    div(class_name="playlist-name", p["name"]),
                    div(class_name="track-count", f"{len(p['tracks'])} tracks"),
                )
                for p in PLAYLISTS
            ])
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"]),
            ul(*[
                li(
                    key=t["id"],
                    class_name=f"track-item {'now-playing' if t['id'] == now_playing_id else ''}",
                    on_click=lambda tid=t["id"]: handle_track_click(tid),
                    div(class_name="track-info",
                        div(class_name="track-title", t["title"]),
                        div(class_name="track-artist", t["artist"]),
                    ),
                    div(class_name="track-duration", t["duration"]),
                )
                for t in active_playlist["tracks"]
            ])
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                div(class_name="now-playing-text",
                    f"{now_playing_track['title']} - {now_playing_track['artist']}" if now_playing_track else "No track selected"
                )
            ),
            button(
                class_name="play-pause-btn",
                on_click=toggle_playback,
                "⏸ Pause" if is_playing else "▶ Play"
            ),
        ),
    )
```
