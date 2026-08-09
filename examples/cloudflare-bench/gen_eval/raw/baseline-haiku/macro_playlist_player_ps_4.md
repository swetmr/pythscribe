<!-- exp=baseline-haiku task=macro_playlist_player cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.05153520000000001 -->
```python
"use client"

from pyths.react import component, use_state

@component
def PlaylistPlayer():
    playlists = [
        {
            "id": "rock",
            "name": "Rock Classics",
            "tracks": [
                {"id": "r1", "title": "Bohemian Rhapsody", "artist": "Queen", "duration": "5:55"},
                {"id": "r2", "title": "Stairway to Heaven", "artist": "Led Zeppelin", "duration": "8:02"},
                {"id": "r3", "title": "Hotel California", "artist": "Eagles", "duration": "6:30"},
            ]
        },
        {
            "id": "pop",
            "name": "Pop Hits",
            "tracks": [
                {"id": "p1", "title": "Blinding Lights", "artist": "The Weeknd", "duration": "3:20"},
                {"id": "p2", "title": "Levitating", "artist": "Dua Lipa", "duration": "3:23"},
                {"id": "p3", "title": "As It Was", "artist": "Harry Styles", "duration": "2:47"},
            ]
        },
        {
            "id": "jazz",
            "name": "Jazz Vibes",
            "tracks": [
                {"id": "j1", "title": "Take Five", "artist": "Dave Brubeck", "duration": "5:24"},
                {"id": "j2", "title": "Blue Monk", "artist": "Thelonious Monk", "duration": "5:48"},
                {"id": "j3", "title": "So What", "artist": "Miles Davis", "duration": "9:02"},
            ]
        },
    ]
    
    active_playlist_id, set_active_playlist_id = use_state("rock")
    now_playing_track_id, set_now_playing_track_id = use_state("r1")
    is_playing, set_is_playing = use_state(False)
    
    active_playlist = next((p for p in playlists if p["id"] == active_playlist_id), playlists[0])
    now_playing_track = next((t for t in active_playlist["tracks"] if t["id"] == now_playing_track_id), None)
    
    def select_playlist(playlist_id):
        set_active_playlist_id(playlist_id)
        playlist = next(p for p in playlists if p["id"] == playlist_id)
        if playlist["tracks"]:
            set_now_playing_track_id(playlist["tracks"][0]["id"])
    
    def select_track(track_id):
        set_now_playing_track_id(track_id)
    
    def toggle_play():
        set_is_playing(not is_playing)
    
    play_button_text = "⏸ Pause" if is_playing else "▶ Play"
    
    now_playing_display = div(class_name="now-playing-info",
        div(class_name="track-title", now_playing_track["title"]),
        div(class_name="track-artist", now_playing_track["artist"]),
    ) if now_playing_track else div(class_name="no-track", "No track selected")
    
    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h3("Playlists"),
            ul(class_name="playlist-list",
                *[li(key=p["id"],
                     class_name="playlist-item" + (" active" if p["id"] == active_playlist_id else ""),
                     on_click=lambda pid=p["id"]: select_playlist(pid),
                     div(class_name="playlist-name", p["name"]),
                     div(class_name="track-count", f"{len(p['tracks'])} tracks"))
                   for p in playlists]),
        ),
        div(class_name="main-panel",
            h3(active_playlist["name"]),
            ul(class_name="tracks-list",
                *[li(key=t["id"],
                     class_name="track-item" + (" now-playing" if t["id"] == now_playing_track_id else ""),
                     on_click=lambda tid=t["id"]: select_track(tid),
                     div(class_name="track-info",
                         div(class_name="track-title", t["title"]),
                         div(class_name="track-artist", t["artist"]),
                         div(class_name="track-duration", t["duration"])))
                  for t in active_playlist["tracks"]]),
        ),
        div(class_name="player-bar",
            now_playing_display,
            button(class_name="play-button",
                   on_click=lambda: toggle_play(),
                   play_button_text),
        ),
    )
```
