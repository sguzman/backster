#!/usr/bin/env wolframscript
(* Rolling P-Value Calculation Script *)

If[Length[$ScriptCommandLine] < 3,
  Print["Usage: rolling_p_values.wls <input_csv> <output_csv> [window_size]"];
  Exit[1];
];

inputPath = $ScriptCommandLine[[2]];
outputPath = $ScriptCommandLine[[3]];
windowSize = If[Length[$ScriptCommandLine] >= 4, ToExpression[$ScriptCommandLine[[4]]], 126];

Print["Loading data from: ", inputPath];
data = Import[inputPath, "Dataset", "HeaderLines" -> 1];

ClearAll[getReturnSeriesFromData, makeRollingWindows, 
  safeEstimatedDistribution, safePValue, rollingDistributionPValues];

getReturnSeriesFromData[ds_] := 
  Module[{dates, prices, rets}, 
   dates = ds[All, 1];
   prices = N[ds[All, 2]];
   rets = Differences[Log[prices]];
   <|"Dates" -> Normal[Rest[dates]], "Returns" -> Normal[rets]|>];

makeRollingWindows[dates_List, vals_List, window_Integer?Positive] := 
  Module[{n = Length[vals]}, 
   Table[<|"EndDate" -> dates[[i + window - 1]], 
     "Data" -> vals[[i ;; i + window - 1]]|>, {i, 1, n - window + 1}]];

safeEstimatedDistribution[data_, family_] := 
  Quiet@Check[EstimatedDistribution[data, family], $Failed];

safePValue[data_, fit_] := 
  Quiet@Check[DistributionFitTest[data, fit, "PValue"], 
    Missing["NotAvailable"]];

rollingDistributionPValues[ds_, distFamilies_List, wSize_] := 
  Module[{parsed, dates, rets, windows, familyNames, scores}, 
   parsed = getReturnSeriesFromData[ds];
   dates = parsed["Dates"];
   rets = parsed["Returns"];
   If[wSize > Length[rets], Return[$Failed]];
   windows = makeRollingWindows[dates, rets, wSize];
   familyNames = {"Normal", "StudentT", "Laplace", "Logistic", "Cauchy"};
   
   Table[
    Module[{wdata = windows[[i, "Data"]], ts = windows[[i, "EndDate"]]},
     Join[<|"timestamp" -> ts|>, 
      AssociationThread[familyNames, 
       Table[
        Module[{fit = safeEstimatedDistribution[wdata, distFamilies[[j]]]},
         If[fit === $Failed, 0.0, safePValue[wdata, fit]]
        ], {j, Length[distFamilies]}]]]
    ], {i, Length[windows]}]];

distFamilies = {NormalDistribution[\[Mu], \[Sigma]], 
   StudentTDistribution[\[Nu], \[Mu], \[Sigma]], 
   LaplaceDistribution[\[Mu], \[Sigma]], 
   LogisticDistribution[\[Mu], \[Beta]], 
   CauchyDistribution[\[Alpha], \[Beta]]};

Print["Calculating rolling p-values (window size: ", windowSize, ")..."];
results = rollingDistributionPValues[data, distFamilies, windowSize];

If[results === $Failed,
  Print["Error: Window size too large for data."];
  Exit[1];
];

Print["Exporting results to: ", outputPath];
Export[outputPath, results];
Print["Done."];
