use crate::domain::{DecisionCandidate, DecisionScore};

pub(super) const AUTO_LABEL_THRESHOLD: f64 = 0.8;

pub(super) fn automatic_label_names(
    scores: &[DecisionScore],
    candidates: &[DecisionCandidate],
) -> Vec<String> {
    scores
        .iter()
        .filter(|score| score.probability >= AUTO_LABEL_THRESHOLD)
        .filter(|score| {
            candidates
                .iter()
                .any(|candidate| candidate.name == score.candidate)
        })
        .map(|score| score.candidate.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::automatic_label_names;
    use crate::domain::{DecisionCandidate, DecisionScore};

    #[test]
    fn rf_wi_1_automatic_labels_apply_the_threshold_and_candidate_boundary() {
        let candidates = [
            DecisionCandidate {
                name: "below".to_owned(),
                description: None,
            },
            DecisionCandidate {
                name: "boundary".to_owned(),
                description: None,
            },
        ];
        let scores = [
            DecisionScore {
                candidate: "below".to_owned(),
                probability: 0.79,
            },
            DecisionScore {
                candidate: "boundary".to_owned(),
                probability: 0.8,
            },
            DecisionScore {
                candidate: "not-offered".to_owned(),
                probability: 1.0,
            },
        ];
        assert_eq!(automatic_label_names(&scores, &candidates), ["boundary"]);
    }
}
