<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0232836 -->
```python
"use client"
R*

INITIAL_TWEETS = [
    {"id": 1, "author": "Ada Lovelace", "text": "The Analytical Engine weaves algebraic patterns, just as the Jacquard loom weaves flowers and leaves.", "likes": 42},
    {"id": 2, "author": "Alan Turing", "text": "Machines take me by surprise with great frequency.", "likes": 17},
    {"id": 3, "author": "Grace Hopper", "text": "The most dangerous phrase in the language is, 'We've always done it this way.'", "likes": 88},
]

MAX_LEN = 280

@c
def TweetFeed():
    tweets, set_tweets = us(INITIAL_TWEETS)
    draft, set_draft = us("")

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": (tweets[0]["id"] + 1) if tweets else 1, "author": "You", "text": draft.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(cn="composer-input", value=draft, ph="What's happening?",
                oh=lambda e: set_draft(e.target.value)),
            div(cn="composer-footer",
                span(cn="char-counter" if remaining >= 0 else "char-counter over-limit", str(remaining)),
                button(cn="post-button", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        div(cn="tweet-list",
            *[div(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda: like(t["id"]), f"♥ {t['likes']}"),
            ) for t in tweets]
        ),
    )
```
