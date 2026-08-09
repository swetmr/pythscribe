<!-- exp=baseline-haiku task=macro_tweet_composer cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.050401299999999996 -->
```python
"use client"
from pyths.react import component, use_state

@component
def TweetFeed():
    initial_tweets = [
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Machine learning is fun", "likes": 15},
        {"id": 3, "author": "Charlie", "text": "Loving PythScribe so far", "likes": 89},
    ]
    
    tweets, set_tweets = use_state(initial_tweets)
    text, set_text = use_state("")
    
    char_limit = 280
    char_remaining = char_limit - len(text)
    can_post = len(text) > 0 and char_remaining >= 0
    
    def post_tweet():
        if can_post:
            new_tweet = {
                "id": max([t["id"] for t in tweets]) + 1 if tweets else 1,
                "author": "You",
                "text": text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
    
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
            h2("Compose Tweet"),
            textarea(
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="What's happening!?",
                class_name="compose-textarea",
                rows=4
            ),
            div(class_name="compose-footer",
                span(class_name="char-counter",
                    f"{char_remaining} characters remaining"),
                button(
                    on_click=post_tweet,
                    disabled=not can_post,
                    class_name="post-button",
                    "Post"
                )
            )
        ),
        div(class_name="tweets-list",
            *[div(key=t["id"], class_name="tweet",
                div(class_name="tweet-header",
                    span(class_name="tweet-author", f"@{t['author']}"),
                ),
                p(class_name="tweet-text", t["text"]),
                div(class_name="tweet-footer",
                    button(
                        on_click=lambda: toggle_like(t["id"]),
                        class_name="like-button",
                        f"❤ {t['likes']}"
                    )
                )
            ) for t in tweets]
        )
    )
```
