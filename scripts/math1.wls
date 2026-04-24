(* Rolling p-value distro fit (legacy) + JSON wrappers for WSTP.

   The core rolling-fit logic below is copied from the original "math1" script
   so results match Wolfram's EstimatedDistribution + DistributionFitTest.

   Rust calls:
     BacksterFinancialDataCloseAndFitJSON["AAPL","2020-01-01","2021-01-01","Close",126]
     BacksterExprCloseAndFitJSON["TimeSeries[...]", 126]
*)

ClearAll[
  getReturnSeriesFromData,
  makeRollingWindows,
  safeEstimatedDistribution,
  safePValue,
  rollingDistributionPValues
];

getReturnSeriesFromData[data_] :=
  Module[{dates, prices, rets},
    dates = data["Dates"];
    prices = N@QuantityMagnitude@data["Values"];
    rets = N@Differences[Log[prices]];
    <|"Dates" -> Rest[dates], "Returns" -> rets|>
  ];

makeRollingWindows[dates_List, vals_List, window_Integer?Positive] :=
  Module[{n = Length[vals]},
    Table[
      <|"EndDate" -> dates[[i + window - 1]], "Data" -> vals[[i ;; i + window - 1]]|>,
      {i, 1, n - window + 1}
    ]
  ];

safeEstimatedDistribution[data_, family_] :=
  Quiet@Check[EstimatedDistribution[data, family], $Failed];

safePValue[data_, fit_] :=
  Quiet@Check[DistributionFitTest[data, fit, "PValue"], Missing["NotAvailable"]];

rollingDistributionPValues[data_, distFamilies_List, windowSize_ : 126] :=
  Module[{parsed, dates, rets, windows, familyNames, scores},
    parsed = getReturnSeriesFromData[data];
    dates = parsed["Dates"];
    rets = parsed["Returns"];
    If[windowSize > Length[rets], Return[$Failed]];
    windows = makeRollingWindows[dates, rets, windowSize];
    familyNames = ToString[InputForm[#]] & /@ distFamilies;
    scores =
      AssociationThread[
        familyNames,
        Table[
          Module[{fam = distFamilies[[j]]},
            Table[
              Module[{wdata, fit, p},
                wdata = windows[[i, "Data"]];
                fit = safeEstimatedDistribution[wdata, fam];
                p = If[fit === $Failed, Missing["FitFailed"], safePValue[wdata, fit]];
                {windows[[i, "EndDate"]], p}
              ],
              {i, Length[windows]}
            ]
          ],
          {j, Length[distFamilies]}
        ]
      ];
    <|"WindowSize" -> windowSize, "Scores" -> scores|>
  ];

(* ---- WSTP-facing wrappers ---- *)

ClearAll[
  BacksterFinancialDataCloseAndFitJSON,
  BacksterExprCloseAndFitJSON,
  backsterCloseRows,
  backsterRollingFitRows,
  backsterMaybeNumber
];

backsterMaybeNumber[x_] := If[NumericQ[x], N[x], Null];

backsterCloseRows[data_] :=
  Module[{dates, values},
    dates = data["Dates"];
    values = N@QuantityMagnitude@data["Values"];
    Transpose[{1000*UnixTime /@ dates, values}]
  ];

backsterRollingFitRows[data_, distFamilies_List, windowSize_Integer?Positive] :=
  Module[{parsed, dates, rets, windows, safeParams, padParams, rows},
    parsed = getReturnSeriesFromData[data];
    dates = parsed["Dates"];
    rets = parsed["Returns"];
    If[windowSize > Length[rets], Return[{}]];
    windows = makeRollingWindows[dates, rets, windowSize];

    (* Deterministic: return numeric distribution parameters for Rust-side sampling.
       This is safe to cache. *)
    (* `List @@ Dist[...]` already returns the parameter list (no head). Do NOT `Rest` it. *)
    safeParams[fit_] := Quiet@Check[List @@ fit, Missing["NotAvailable"]];
    padParams[p_, n_] := If[ListQ[p], PadRight[p, n, Null], ConstantArray[Null, n]];

    rows =
      Table[
        Module[{wdata, end, fits, ps0, params0, ps, params},
          wdata = windows[[i, "Data"]];
          end = windows[[i, "EndDate"]];
          fits = safeEstimatedDistribution[wdata, #] & /@ distFamilies;

          ps0 = If[# === $Failed, Missing["FitFailed"], safePValue[wdata, #]] & /@ fits;
          params0 = If[# === $Failed, Missing["FitFailed"], safeParams[#]] & /@ fits;

          ps = backsterMaybeNumber /@ ps0;
          params = (If[ListQ[#], backsterMaybeNumber /@ #, #] &) /@ params0;

          (* Fixed layout (17 values). Always stable length, using Null for missing. *)
          Join[
            {1000*UnixTime[end]},
            {ps[[1]]}, padParams[params[[1]], 2],
            {ps[[2]]}, padParams[params[[2]], 3],
            {ps[[3]]}, padParams[params[[3]], 2],
            {ps[[4]]}, padParams[params[[4]], 2],
            {ps[[5]]}, padParams[params[[5]], 2]
          ]
        ],
        {i, Length[windows]}
      ];

    rows
  ];

distFamilies = {
  NormalDistribution[\[Mu], \[Sigma]],
  StudentTDistribution[\[Nu], \[Mu], \[Sigma]],
  LaplaceDistribution[\[Mu], \[Sigma]],
  LogisticDistribution[\[Mu], \[Beta]],
  CauchyDistribution[\[Alpha], \[Beta]]
};

BacksterFinancialDataCloseAndFitJSON[
  symbol_String,
  start_String,
  end_String,
  field_String : "Close",
  windowSize_Integer : 126
] :=
  Module[{ts, closeRows, fitRows},
    ts =
      TimeConstrained[
        Quiet@Check[
          FinancialData[
            symbol,
            {DateObject[start], DateObject[end]},
            field
          ],
          $Failed
        ],
        20,
        $Failed
      ];
    If[ts === $Failed, Return[ExportString[<|"close" -> {}, "fit" -> {}|>, "JSON"]]];

    closeRows = backsterCloseRows[ts];
    If[windowSize <= 0, Return[ExportString[<|"close" -> closeRows, "fit" -> {}|>, "JSON"]]];
    fitRows = backsterRollingFitRows[ts, distFamilies, windowSize];

    ExportString[<|"close" -> closeRows, "fit" -> fitRows|>, "JSON"]
  ];

BacksterExprCloseAndFitJSON[
  expr_String,
  windowSize_Integer : 126
] :=
  Module[{ts, closeRows, fitRows},
    ts =
      TimeConstrained[
        Quiet@Check[ToExpression[expr], $Failed],
        20,
        $Failed
      ];
    If[ts === $Failed, Return[ExportString[<|"close" -> {}, "fit" -> {}|>, "JSON"]]];

    closeRows = backsterCloseRows[ts];
    If[windowSize <= 0, Return[ExportString[<|"close" -> closeRows, "fit" -> {}|>, "JSON"]]];
    fitRows = backsterRollingFitRows[ts, distFamilies, windowSize];

    ExportString[<|"close" -> closeRows, "fit" -> fitRows|>, "JSON"]
  ];
