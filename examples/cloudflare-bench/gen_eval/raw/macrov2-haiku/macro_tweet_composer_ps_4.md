<!-- exp=macrov2-haiku task=macro_tweet_composer cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.020795799999999996 -->
```python
from pyths.react import component, use_state

@component
def TweetFeed():
    tweets, set_tweets = use_state([
        {"author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"author": "Bob", "text": "Machine learning is fascinating", "likes": 15},
        {"author": "Charlie", "text": "Building cool things with Python", "likes": 28},
    ])
    
    compose_text, set_compose_text = use_state("")
    
    char_count = len(compose_text)
    remaining = 280 - char_count
    is_over_limit = char_count > 280
    is_empty = char_count == 0
    can_post = not is_empty and not is_over_limit
    
    def post_tweet():
        if can_post:
            new_tweet = {"author": "You", "text": compose_text, "likes": 0}
            set_tweets([new_tweet, *tweets])
            set_compose_text("")
    
    def like_tweet(index):
        updated = [t for t in tweets]
        updated[index] = {**updated[index], "likes": updated[index]["likes"] + 1}
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
                span(class_name=f"char-count {'over-limit' if is_over_limit else ''}", 
                     f"{remaining}"),
                button(
                    on_click=lambda: post_tweet(),
                    disabled=not can_post,
                    class_name="post-button",
                    "Post"
                )
            )
        ),
        div(class_name="tweets-list",
            *[div(key=i, class_name="tweet",
                div(class_name="tweet-header",
                    span(class_name="author", tweet["author"]),
                    span(class_name="timestamp", "now")
                ),
                p(class_name="tweet-text", tweet["text"]),
                div(class_name="tweet-actions",
                    button(
                        on_click=lambda: like_tweet(i),
                        class_name="like-button",
                        f"❤ {tweet['likes']}"
                    )
                )
            ) for i, tweet in enumerate(tweets)]
        )
    )
```
