<!-- exp=macrov2-haiku task=macro_tweet_composer cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.019455800000000002 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TweetFeed():
    initial_tweets = [
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "PythScribe is amazing!", "likes": 15},
        {"id": 3, "author": "Charlie", "text": "Building cool things with React", "likes": 28},
    ]
    
    tweets, set_tweets = use_state(initial_tweets)
    compose_text, set_compose_text = use_state("")
    
    char_limit = 280
    remaining = char_limit - len(compose_text)
    can_post = len(compose_text) > 0 and remaining >= 0
    
    def post_tweet():
        if can_post:
            new_tweet = {
                "id": max([t["id"] for t in tweets], default=0) + 1,
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
            h2("Compose Tweet"),
            textarea(
                value=compose_text,
                on_change=lambda e: set_compose_text(e.target.value),
                placeholder="What's on your mind?",
                class_name="compose-textarea"
            ),
            div(class_name="compose-footer",
                span(
                    class_name=f"char-counter {'' if remaining >= 0 else 'over-limit'}",
                    f"{remaining} characters remaining"
                ),
                button(
                    on_click=lambda: post_tweet(),
                    disabled=not can_post,
                    class_name="post-button",
                    "Post Tweet"
                )
            )
        ),
        div(class_name="tweets-list",
            *[
                div(key=t["id"], class_name="tweet",
                    div(class_name="tweet-header",
                        span(class_name="author", t["author"])
                    ),
                    div(class_name="tweet-text", t["text"]),
                    div(class_name="tweet-footer",
                        button(
                            on_click=lambda: like_tweet(t["id"]),
                            class_name="like-button",
                            f"❤️ {t['likes']}"
                        )
                    )
                )
                for t in tweets
            ]
        )
    )
```
