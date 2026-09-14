"""
Advanced Machine Learning (AML) Subagent Pipeline
Runs automated model selection and hyperparameter evaluation.
"""

import math
import numpy as np
from typing import Dict, List, Any, Tuple
from dataclasses import dataclass

@dataclass
class ModelCandidate:
    model_id: int
    name: str
    params: Dict[str, Any]
    score: float = 0.0
    training_time_ms: float = 0.0

class AMLSpiralParameterGenerator:
    """Uses golden-ratio spiral geometry to sample hyperparameter space."""
    
    @staticmethod
    def sample_parameters(num_candidates: int) -> List[Dict[str, Any]]:
        candidates = []
        phi = (1 + math.sqrt(5)) / 2  # Golden ratio
        
        for i in range(1, num_candidates + 1):
            radius = math.log(i + 1) * phi
            angle = i * 2 * math.pi * phi
            
            # Map spiral coordinates to hyperparameters
            learning_rate = round(10 ** (-1 - (abs(math.cos(angle)) * 2)), 4)
            max_depth = max(3, int(3 + radius * 1.5))
            n_estimators = int(50 + radius * 30)
            
            candidates.append({
                "candidate_id": i,
                "learning_rate": learning_rate,
                "max_depth": max_depth,
                "n_estimators": n_estimators
            })
        return candidates

class AMLEnsembleEvaluator:
    """Aggregates subagent model predictions into a meta-ensemble score."""
    
    def __init__(self, candidates: List[ModelCandidate]):
        self.candidates = candidates

    def build_ensemble(self, top_k: int = 3) -> Dict[str, Any]:
        # Sort by validation score descending
        sorted_models = sorted(self.candidates, key=lambda m: m.score, reverse=True)
        top_models = sorted_models[:top_k]
        
        # Calculate soft-voting weights based on normalized validation score
        total_score = sum(m.score for m in top_models)
        weights = {m.model_id: round(m.score / total_score, 4) for m in top_models}
        
        ensemble_score = sum(m.score * weights[m.model_id] for m in top_models)
        
        return {
            "top_model_count": len(top_models),
            "ensemble_score": round(ensemble_score, 4),
            "model_weights": weights,
            "best_candidate": top_models[0] if top_models else None
        }

# Example execution within cothink-system subagent worker
if __name__ == "__main__":
    print("=== Advanced Machine Learning (AML) Agent Subagent ===")
    
    # 1. Generate candidate hyperparameters along spiral
    params_list = AMLSpiralParameterGenerator.sample_parameters(num_candidates=6)
    
    # 2. Simulate parallel subagent model evaluation
    trained_candidates = []
    for p in params_list:
        # Simulated validation score (e.g. F1-score / Accuracy)
        simulated_score = 0.85 + (0.10 * math.sin(p["candidate_id"])) - (p["learning_rate"] * 0.2)
        trained_candidates.append(
            ModelCandidate(
                model_id=p["candidate_id"],
                name=f"XGBoost_Candidate_{p['candidate_id']}",
                params=p,
                score=max(0.50, min(0.99, round(simulated_score, 4))),
                training_time_ms=round(120.0 + p["n_estimators"] * 1.2, 2)
            )
        )
    
    # 3. Build Stacking Ensemble
    evaluator = AMLEnsembleEvaluator(trained_candidates)
    ensemble = evaluator.build_ensemble(top_k=3)
    
    print(f"\n[AML Agent] Evaluated {len(trained_candidates)} parallel models.")
    print(f"[AML Agent] Optimal Ensemble Score: {ensemble['ensemble_score']}")
    print(f"[AML Agent] Model Weights: {ensemble['model_weights']}")
    if ensemble["best_candidate"]:
        best = ensemble["best_candidate"]
        print(f"[AML Agent] Best Single Model: {best.name} (Score: {best.score}) Params: {best.params}")

