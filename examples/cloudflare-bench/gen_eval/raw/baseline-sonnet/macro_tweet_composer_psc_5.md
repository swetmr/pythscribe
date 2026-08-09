<!-- exp=baseline-sonnet task=macro_tweet_composer cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.0433113 -->
```python
"use client"
R*

def make_tweet(author, text, likes):
    return {"author": author, "text": text, "likes": likes}

SEED_TWEETS = [
    make_tweet("ada", "Hello, world! First tweet from the analytical engine.", 12),
    make_tweet("grace", "Debugging is like being the detective in a crime movie.", 8),
    make_tweet("linus", "Talk is cheap. Show me the code.", 42),
]

@c
def TweetFeed():
    tweets, set_tweets = us([{**t, "id": i} for i, t in enumerate(SEED_TWEETS)])
    draft, set_draft = us("")
    next_id, set_next_id = us(len(SEED_TWEETS))

    remaining = 280 - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": next_id, "author": "you", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")
        set_next_id(next_id + 1)

    def like(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(cn="composer-input", value=draft, ph="What's happening?",
                     oh=lambda e: set_draft(e.target.value)),
            div(cn="composer-footer",
                span(cn="char-counter" + (" over-limit" if remaining < 0 else ""), str(remaining)),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        div(cn="tweet-list",
            *[div(key=t["id"], cn="tweet",
                div(cn="tweet-author", f"@{t['author']}"),
                div(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda tid=t["id"]: like(tid), f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
