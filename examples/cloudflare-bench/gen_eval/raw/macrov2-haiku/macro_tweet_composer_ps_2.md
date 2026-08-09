<!-- exp=macrov2-haiku task=macro_tweet_composer cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0303058 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TweetFeed():
    initial_tweets = [
        {"author": "Alice", "text": "Just finished my first PythScribe project!", "likes": 42},
        {"author": "Bob", "text": "PythScribe is amazing, Python in the browser! 🚀", "likes": 28},
        {"author": "Charlie", "text": "Building reactive UIs has never been easier", "likes": 15},
    ]
    
    tweets, set_tweets = use_state(initial_tweets)
    compose_text, set_compose_text = use_state("")
    
    remaining = 280 - len(compose_text)
    can_post = len(compose_text) > 0 and remaining >= 0
    
    def post_tweet():
        if can_post:
            new_tweet = {
                "author": "You",
                "text": compose_text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_compose_text("")
    
    def toggle_like(index):
        updated = list(tweets)
        updated[index] = {**updated[index], "likes": updated[index]["likes"] + 1}
        set_tweets(updated)
    
    return div(class_name="tweet-feed",
        div(class_name="compose-section",
            textarea(
                value=compose_text,
                on_change=lambda e: set_compose_text(e.target.value),
                placeholder="What's happening!?",
                class_name="compose-textarea",
                rows="3"
            ),
            div(class_name="compose-footer",
                span(
                    class_name="char-counter",
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
            *[div(class_name="tweet", key=i,
                div(class_name="tweet-header",
                    span(class_name="author", tweet["author"])
                ),
                p(class_name="tweet-text", tweet["text"]),
                div(class_name="tweet-actions",
                    button(
                        on_click=lambda: toggle_like(i),
                        class_name="like-button",
                        f"❤️ {tweet['likes']}"
                    )
                )
            ) for i, tweet in enumerate(tweets)]
        )
    )
```
