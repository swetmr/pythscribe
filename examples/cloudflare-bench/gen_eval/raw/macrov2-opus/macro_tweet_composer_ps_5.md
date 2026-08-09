<!-- exp=macrov2-opus task=macro_tweet_composer cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.032478 -->
```python
"use client"

from pyths.react import component, use_state

MAX_CHARS = 280

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just shipped a new compiler pass. Feeling great!", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Debugging is like being the detective in a crime movie where you are also the murderer.", "likes": 42},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 128},
]


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    text, set_text = use_state("")

    remaining = MAX_CHARS - len(text)
    over_limit = remaining < 0
    can_post = len(text.strip()) > 0 and not over_limit

    next_id = (max([t["id"] for t in tweets]) + 1) if tweets else 1

    def post():
        if not can_post:
            return
        new_tweet = {"id": next_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    counter_class = "char-counter over" if over_limit else "char-counter"

    return div(class_name="tweet-feed",
        h2("Home"),
        div(class_name="composer",
            textarea(
                class_name="composer-input",
                value=text,
                placeholder="What's happening?",
                on_change=lambda e: set_text(e.target.value),
            ),
            div(class_name="composer-actions",
                span(class_name=counter_class, f"{remaining}"),
                button(
                    class_name="post-button",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                div(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda: like(t["id"]),
                    f"♥ {t['likes']}",
                ),
              ) for t in tweets]
        ),
    )
```
