use crate::error::{Error, Result};
use crate::event::Event;
use serde::{Deserialize, Deserializer, Serialize};
//use serde_json::json;
//use serde_json::Result;

#[derive(Serialize, PartialEq, Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub filters: Vec<ReqFilter>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReqFilter {
    pub id: Option<String>,
    pub author: Option<String>,
    pub kind: Option<u64>,
    #[serde(rename = "e#")]
    pub event: Option<String>,
    #[serde(rename = "p#")]
    pub pubkey: Option<String>,
    pub since: Option<u64>,
    pub authors: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for Subscription {
    fn deserialize<D>(deserializer: D) -> Result<Subscription, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut v: serde_json::Value = Deserialize::deserialize(deserializer)?;
        // this shoud be a 3-or-more element array.
        // verify the first element is a String, REQ
        // get the subscription from the second element.
        // convert each of the remaining objects into filters

        // check for array
        let va = v
            .as_array_mut()
            .ok_or(serde::de::Error::custom("not array"))?;

        // check length
        if va.len() < 3 {
            return Err(serde::de::Error::custom("not enough fields"));
        }
        let mut i = va.into_iter();
        // get command ("REQ") and ensure it is a string
        let req_cmd_str: serde_json::Value = i.next().unwrap().take();
        let req = req_cmd_str.as_str().ok_or(serde::de::Error::custom(
            "first element of request was not a string",
        ))?;
        if req != "REQ" {
            return Err(serde::de::Error::custom("missing REQ command"));
        }

        // ensure sub id is a string
        let sub_id_str: serde_json::Value = i.next().unwrap().take();
        let sub_id = sub_id_str
            .as_str()
            .ok_or(serde::de::Error::custom("missing subscription id"))?;

        let mut filters = vec![];
        for fv in i {
            let f: ReqFilter = serde_json::from_value(fv.take())
                .map_err(|_| serde::de::Error::custom("could not parse filter"))?;
            filters.push(f);
        }
        Ok(Subscription {
            id: sub_id.to_owned(),
            filters,
        })
    }
}

impl Subscription {
    pub fn parse(json: &str) -> Result<Subscription> {
        serde_json::from_str(json).map_err(|e| Error::JsonParseFailed(e))
    }
    pub fn get_id(&self) -> String {
        self.id.clone()
    }
    pub fn get_filter_count(&self) -> usize {
        self.filters.len()
    }
    pub fn interested_in_event(&self, event: &Event) -> bool {
        // loop through every filter, and return true if any match this event.
        for f in self.filters.iter() {
            if f.interested_in_event(event) {
                return true;
            }
        }
        return false;
    }
}

impl ReqFilter {
    // attempt to match against author/authors fields
    fn author_match(&self, event: &Event) -> bool {
        self.authors
            .as_ref()
            .map(|vs| vs.contains(&event.pubkey.to_owned()))
            .unwrap_or(true)
            && self
                .author
                .as_ref()
                .map(|v| v == &event.pubkey)
                .unwrap_or(true)
    }
    fn event_match(&self, event: &Event) -> bool {
        self.event
            .as_ref()
            .map(|t| event.event_tag_match(t))
            .unwrap_or(true)
    }

    fn kind_match(&self, kind: u64) -> bool {
        self.kind.map(|v| v == kind).unwrap_or(true)
    }

    pub fn interested_in_event(&self, event: &Event) -> bool {
        // determine if all populated fields in this filter match the provided event.
        // a filter matches an event if all the populated fields match.
        self.id.as_ref().map(|v| v == &event.id).unwrap_or(true)
            && self.since.map(|t| event.created_at > t).unwrap_or(true)
            && self.kind_match(event.kind)
            && self.author_match(&event)
            && self.event_match(&event)
            && true // match if all other fields are absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_request_parse() -> Result<()> {
        let raw_json = "[\"REQ\",\"some-id\",{}]";
        let s: Subscription = serde_json::from_str(raw_json)?;
        assert_eq!(s.id, "some-id");
        assert_eq!(s.filters.len(), 1);
        assert_eq!(s.filters.get(0).unwrap().author, None);
        Ok(())
    }

    #[test]
    fn multi_empty_request_parse() -> Result<()> {
        let raw_json = r#"["REQ","some-id",{}]"#;
        let s: Subscription = serde_json::from_str(raw_json)?;
        assert_eq!(s.id, "some-id");
        assert_eq!(s.filters.len(), 1);
        assert_eq!(s.filters.get(0).unwrap().author, None);
        Ok(())
    }

    #[test]
    fn incorrect_header() {
        let raw_json = "[\"REQUEST\",\"some-id\",\"{}\"]";
        assert!(serde_json::from_str::<Subscription>(raw_json).is_err());
    }

    #[test]
    fn req_missing_filters() {
        let raw_json = "[\"REQ\",\"some-id\"]";
        assert!(serde_json::from_str::<Subscription>(raw_json).is_err());
    }

    #[test]
    fn invalid_filter() {
        // unrecognized field in filter
        let raw_json = "[\"REQ\",\"some-id\",{\"foo\": 3}]";
        assert!(serde_json::from_str::<Subscription>(raw_json).is_err());
    }

    #[test]
    fn author_filter() -> Result<()> {
        let raw_json = "[\"REQ\",\"some-id\",{\"author\": \"test-author-id\"}]";
        let s: Subscription = serde_json::from_str(raw_json)?;
        assert_eq!(s.id, "some-id");
        assert_eq!(s.filters.len(), 1);
        let first_filter = s.filters.get(0).unwrap();
        assert_eq!(first_filter.author, Some("test-author-id".to_owned()));
        Ok(())
    }

    #[test]
    fn interest_id_nomatch() -> Result<()> {
        // subscription with a filter for ID
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"id":"abc"}]"#)?;
        let e = Event {
            id: "abcde".to_owned(),
            pubkey: "".to_owned(),
            created_at: 0,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), false);
        Ok(())
    }

    #[test]
    fn interest_time_and_id() -> Result<()> {
        // subscription with a filter for ID and time
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"id":"abc", "since": 1000}]"#)?;
        let e = Event {
            id: "abc".to_owned(),
            pubkey: "".to_owned(),
            created_at: 50,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), false);
        Ok(())
    }

    #[test]
    fn interest_time_and_id2() -> Result<()> {
        // subscription with a filter for ID and time
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"id":"abc", "since": 1000}]"#)?;
        let e = Event {
            id: "abc".to_owned(),
            pubkey: "".to_owned(),
            created_at: 1001,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), true);
        Ok(())
    }

    #[test]
    fn interest_id() -> Result<()> {
        // subscription with a filter for ID
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"id":"abc"}]"#)?;
        let e = Event {
            id: "abc".to_owned(),
            pubkey: "".to_owned(),
            created_at: 0,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), true);
        Ok(())
    }

    #[test]
    fn author_single() -> Result<()> {
        // subscription with a filter for ID
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"author":"abc"}]"#)?;
        let e = Event {
            id: "123".to_owned(),
            pubkey: "abc".to_owned(),
            created_at: 0,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), true);
        Ok(())
    }

    #[test]
    fn authors_single() -> Result<()> {
        // subscription with a filter for ID
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"authors":["abc"]}]"#)?;
        let e = Event {
            id: "123".to_owned(),
            pubkey: "abc".to_owned(),
            created_at: 0,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), true);
        Ok(())
    }
    #[test]
    fn authors_multi_pubkey() -> Result<()> {
        // check for any of a set of authors, against the pubkey
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"authors":["abc", "bcd"]}]"#)?;
        let e = Event {
            id: "123".to_owned(),
            pubkey: "bcd".to_owned(),
            created_at: 0,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), true);
        Ok(())
    }

    #[test]
    fn authors_multi_no_match() -> Result<()> {
        // check for any of a set of authors, against the pubkey
        let s: Subscription = serde_json::from_str(r#"["REQ","xyz",{"authors":["abc", "bcd"]}]"#)?;
        let e = Event {
            id: "123".to_owned(),
            pubkey: "xyz".to_owned(),
            created_at: 0,
            kind: 0,
            tags: Vec::new(),
            content: "".to_owned(),
            sig: "".to_owned(),
        };
        assert_eq!(s.interested_in_event(&e), false);
        Ok(())
    }
}