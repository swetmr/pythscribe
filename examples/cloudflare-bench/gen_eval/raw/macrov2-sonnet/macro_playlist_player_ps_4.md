<!-- exp=macrov2-sonnet task=macro_playlist_player cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0520854 -->
```python
"use client"

from pyths.react import component, use_state

def mk_select(select_playlist, playlist_id):
    return lambda: select_playlist(playlist_id)

def mk_play(play_track, track):
    return lambda: play_track(track)

@component
def PlaylistPlayer():
    playlists = [
        {
            "id": 1,
            "name": "Chill Vibes",
            "tracks": [
                {"title": "Sunset Drift", "artist": "Nora Lane", "duration": "3:24"},
                {"title": "Slow Tide", "artist": "Marin Cole", "duration": "4:02"},
                {"title": "Paper Clouds", "artist": "Ezra Wren", "duration": "2:58"},
            ],
        },
        {
            "id": 2,
            "name": "Workout Mix",
            "tracks": [
                {"title": "Overdrive", "artist": "Kade Storm", "duration": "3:12"},
                {"title": "Pulse Check", "artist": "Rina Vex", "duration": "3:47"},
                {"title": "Iron Grip", "artist": "Dax Halo", "duration": "2:39"},
                {"title": "Redline", "artist": "Kade Storm", "duration": "3:55"},
            ],
        },
        {
            "id": 3,
            "name": "Focus Flow",
            "tracks": [
                {"title": "Quiet Signal", "artist": "Ori Sol", "duration": "5:10"},
                {"title": "Deep Work", "artist": "Ivy Chen", "duration": "4:33"},
                {"title": "Still Water", "artist": "Ori Sol", "duration": "3:48"},
            ],
        },
    ]

    active_id, set_active_id = use_state(playlists[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active_playlist = [p for p in playlists if p["id"] == active_id][0]

    def select_playlist(playlist_id):
        set_active_id(playlist_id)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h2("Playlists"),
            ul(*[
                li(
                    key=p["id"],
                    class_name="playlist-item active" if p["id"] == active_id else "playlist-item",
                    on_click=mk_select(select_playlist, p["id"]),
                    div(class_name="playlist-name", p["name"]),
                    div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
                )
                for p in playlists
            ]),
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"]),
            ul(class_name="track-list", *[
                li(
                    key=f"{active_playlist['id']}-{i}",
                    class_name="track-item now-playing" if now_playing and now_playing["title"] == t["title"] and now_playing["artist"] == t["artist"] else "track-item",
                    on_click=mk_play(play_track, t),
                    span(class_name="track-title", t["title"]),
                    span(class_name="track-artist", t["artist"]),
                    span(class_name="track-duration", t["duration"]),
                )
                for i, t in enumerate(active_playlist["tracks"])
            ]),
        ),
        div(class_name="now-playing-bar",
            span(class_name="now-playing-title", now_playing["title"] if now_playing else "No track playing"),
            button(
                class_name="play-pause-btn",
                on_click=lambda: toggle_play(),
                disabled=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
