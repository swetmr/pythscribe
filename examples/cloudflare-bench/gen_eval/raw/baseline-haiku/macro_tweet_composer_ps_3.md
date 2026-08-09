<!-- exp=baseline-haiku task=macro_tweet_composer cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0340286 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TweetFeed():
    tweets, set_tweets = use_state([
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Learning PythScribe is amazing", "likes": 15},
        {"id": 3, "author": "Charlie", "text": "React components in Python? Yes please!", "likes": 89},
    ])
    
    compose_text, set_compose_text = use_state("")
    
    char_count = len(compose_text)
    remaining = 280 - char_count
    can_post = char_count > 0 and char_count <= 280
    
    def post_tweet():
        if can_post:
            new_id = max([t["id"] for t in tweets], default=0) + 1
            new_tweet = {
                "id": new_id,
                "author": "You",
                "text": compose_text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_compose_text("")
    
    def like_tweet(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(class_name="tweet-feed",
        div(class_name="compose-section",
            textarea(
                value=compose_text,
                on_change=lambda e: set_compose_text(e.target.value),
                placeholder="What's happening!?",
                class_name="compose-textarea"
            ),
            div(class_name="compose-footer",
                span(
                    class_name="char-count" + (" over-limit" if remaining < 0 else ""),
                    f"{remaining}"
                ),
                button(
                    on_click=lambda: post_tweet(),
                    disabled=not can_post,
                    class_name="post-button",
                    "Post"
                )
            )
        ),
        div(class_name="tweets-list",
            *[div(key=t["id"], class_name="tweet",
                  div(class_name="tweet-header",
                      span(class_name="author", t["author"])
                  ),
                  div(class_name="tweet-text", t["text"]),
                  div(class_name="tweet-footer",
                      button(
                          on_click=lambda: like_tweet(t["id"]),
                          class_name="like-button",
                          f"♥ {t['likes']}"
                      )
                  )
              )
              for t in tweets]
        )
    )
```
