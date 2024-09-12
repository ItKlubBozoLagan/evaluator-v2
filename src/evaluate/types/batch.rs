use crate::evaluate::compilation::process_compilation_step;
use crate::evaluate::{EvaluationError, SuccessfulEvaluation};
use crate::messages::Evaluation;

pub fn evaluate(evaluation: &Evaluation) -> Result<SuccessfulEvaluation, EvaluationError> {
    let compilation_result = process_compilation_step(evaluation.code.as_str(), &evaluation.language)?;

    // let running_process = compilation_result.process.run()?;
    
    // TODO: figure out checker

    todo!()
}