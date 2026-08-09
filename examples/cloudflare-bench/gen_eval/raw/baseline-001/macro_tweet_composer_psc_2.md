<!-- exp=baseline-001 task=macro_tweet_composer cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.031631 -->
```python
"use client"
R*

def seed_tweets():
    return [
        {"id": 1, "author": "@ada", "text": "Just shipped a new compiler pass. Feels good.", "likes": 12},
        {"id": 2, "author": "@grace", "text": "Debugging is twice as hard as writing the code in the first place.", "likes": 34},
        {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 88},
    ]

MAX_LEN = 280

@c
def TweetFeed():
    tweets, set_tweets = us(seed_tweets())
    text, set_text = us("")

    remaining = MAX_LEN - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_id = (max([t["id"] for t in tweets]) + 1) if tweets else 1
        new_tweet = {"id": new_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t for t in tweets])

    counter_class = "char-counter over" if remaining < 0 else "char-counter"

    return div(cn="tweet-feed",
        h2("Home"),
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
            ),
            div(cn="composer-bar",
                span(cn=counter_class, str(remaining)),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Tweet"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}"),
            ) for t in tweets]),
    )
```
