<!-- exp=baseline-haiku task=macro_tweet_composer cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0339036 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TweetFeed():
    tweets, set_tweets = use_state([
        {"id": 1, "author": "Alice", "text": "Just got back from a great hike!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Python is awesome", "likes": 128},
        {"id": 3, "author": "Carol", "text": "Building something cool with PythScribe", "likes": 87},
    ])
    
    compose_text, set_compose_text = use_state("")
    next_id, set_next_id = use_state(4)
    
    char_count = len(compose_text)
    remaining = 280 - char_count
    can_post = char_count > 0 and char_count <= 280
    
    def post_tweet():
        if can_post:
            new_tweet = {
                "id": next_id,
                "author": "You",
                "text": compose_text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_compose_text("")
            set_next_id(next_id + 1)
    
    def toggle_like(tweet_id):
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
                placeholder="What's happening?!",
                class_name="compose-textarea"
            ),
            div(class_name="compose-footer",
                span(class_name=f"char-counter {'' if remaining >= 0 else 'error'}", 
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
            *[div(class_name="tweet", key=t["id"],
                  div(class_name="tweet-header",
                      span(class_name="tweet-author", t["author"]),
                      span(class_name="tweet-meta", "• now")
                  ),
                  p(class_name="tweet-text", t["text"]),
                  div(class_name="tweet-actions",
                      button(
                          on_click=lambda e, tid=t["id"]: toggle_like(tid),
                          class_name="like-button",
                          f"❤️ {t['likes']}"
                      )
                  )
              )
              for t in tweets]
        )
    )
```
