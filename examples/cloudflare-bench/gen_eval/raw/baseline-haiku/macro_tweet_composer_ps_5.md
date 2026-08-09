<!-- exp=baseline-haiku task=macro_tweet_composer cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0515486 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TweetFeed():
    text, set_text = use_state("")
    
    initial_tweets = [
        {"id": 1, "author": "Alice", "text": "Just launched my new project! 🚀", "likes": 42},
        {"id": 2, "author": "Bob", "text": "PythScribe is amazing for React development", "likes": 28},
        {"id": 3, "author": "Charlie", "text": "What's everyone working on today?", "likes": 15},
    ]
    
    tweets, set_tweets = use_state(initial_tweets)
    
    char_limit = 280
    remaining = char_limit - len(text)
    can_post = len(text) > 0 and remaining >= 0
    
    def post_tweet():
        if can_post:
            new_id = max([t["id"] for t in tweets] + [0]) + 1
            new_tweet = {
                "id": new_id,
                "author": "You",
                "text": text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
    
    def like_tweet(tweet_id):
        updated_tweets = []
        for t in tweets:
            if t["id"] == tweet_id:
                updated_tweets.append({**t, "likes": t["likes"] + 1})
            else:
                updated_tweets.append(t)
        set_tweets(updated_tweets)
    
    return div(class_name="tweet-feed",
        div(class_name="compose-section",
            textarea(
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="What's happening!?",
                class_name="compose-textarea"
            ),
            div(class_name="compose-footer",
                span(
                    class_name=f"char-counter {'' if remaining >= 0 else 'over-limit'}",
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
                        on_click=lambda tid=t["id"]: like_tweet(tid),
                        class_name="like-button",
                        f"♥ {t['likes']}"
                    )
                )
            ) for t in tweets]
        )
    )
```
