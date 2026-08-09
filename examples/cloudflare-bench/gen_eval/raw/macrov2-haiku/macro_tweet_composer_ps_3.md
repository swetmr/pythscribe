<!-- exp=macrov2-haiku task=macro_tweet_composer cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0347208 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TweetFeed():
    tweets, set_tweets = use_state([
        {"id": 1, "author": "Alice", "text": "Just finished an amazing project! 🎉", "likes": 3},
        {"id": 2, "author": "Bob", "text": "React is awesome", "likes": 7},
        {"id": 3, "author": "Carol", "text": "Building cool things with PythScribe", "likes": 12},
    ])
    
    compose_text, set_compose_text = use_state("")
    next_id, set_next_id = use_state(4)
    
    char_count = len(compose_text)
    remaining = 280 - char_count
    can_post = 0 < char_count <= 280
    
    def post_tweet():
        if can_post:
            new_tweet = {
                "id": next_id,
                "author": "You",
                "text": compose_text,
                "likes": 0,
            }
            set_tweets([new_tweet, *tweets])
            set_next_id(next_id + 1)
            set_compose_text("")
    
    def toggle_like(tweet_id):
        updated = []
        for t in tweets:
            if t["id"] == tweet_id:
                updated.append({**t, "likes": t["likes"] + 1})
            else:
                updated.append(t)
        set_tweets(updated)
    
    return div(class_name="tweet-feed",
        div(class_name="compose-section",
            textarea(
                class_name="compose-textarea",
                value=compose_text,
                on_change=lambda e: set_compose_text(e.target.value),
                placeholder="What's happening!?",
                rows=3,
            ),
            div(class_name="compose-controls",
                div(
                    class_name="char-counter",
                    f"{remaining} characters remaining",
                    style={"color": "red" if remaining < 0 else "gray"},
                ),
                button(
                    class_name="post-button",
                    on_click=lambda: post_tweet(),
                    disabled=not can_post,
                    "Post",
                ),
            ),
        ),
        div(class_name="tweets-section",
            *[div(class_name="tweet", key=t["id"],
                div(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda: toggle_like(t["id"]),
                    f"❤️ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```
